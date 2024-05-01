use ahash::AHashMap;
use colourado::{ColorPalette, PaletteType};
use derivative::Derivative;
use figment::providers::{Env, Format, Toml};
use figment::Figment;
use std::ffi::{CStr, OsStr};
use std::process::{Command, ExitStatus};

use graph_format::*;
use neo4rs::{query, ConfigBuilder, Graph};
use serde::Deserialize;
use std::io::{BufRead, BufReader, Write};
use std::sync::Mutex;

#[derive(Deserialize, Derivative)]
#[derivative(Default, Debug)]
#[serde(default)]
struct Config {
    #[derivative(Default(value = "\"127.0.0.1:7687\".to_string()"))]
    uri: String,
    #[derivative(Default(value = "\"neo4j\".to_string()"))]
    user: String,
    #[derivative(Default(value = "\"password\".to_string()"), Debug = "ignore")]
    pass: String,
    #[derivative(Default(value = "5"))]
    min_degree: u32,
    #[derivative(Default(value = "100"))]
    layout_iterations: usize,
    #[derivative(Default(value = "8"))]
    threads: usize,
    #[derivative(Default(value = "1024"))]
    chunk_size: usize,
    #[derivative(Default(value = "0.01"))]
    community_min_gain: f32,
    only_bfs: bool,
}

static LAST_LOG_TIME: Mutex<std::time::Instant> =
    Mutex::new(unsafe { std::mem::transmute([0u8; std::mem::size_of::<std::time::Instant>()]) });

#[macro_export]
macro_rules! log
{
    ($($arg:tt)*) =>
    {
        {
            let mut last_log_time = $crate::LAST_LOG_TIME.lock().unwrap();
            let now = std::time::Instant::now();
            let elapsed = now - *last_log_time;
            *last_log_time = now;
            println!("[{}] [{:>5}ms] [{}:{}] {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S.%3f"),
                elapsed.as_millis(),
                file!(), line!(), format_args!($($arg)*));
        }
    }
}

fn run_command(cmd: &mut Command) -> ExitStatus {
    let mut res = cmd.stdout(std::process::Stdio::piped()).spawn().unwrap();
    if let Some(stdout) = res.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            log!(">>> {}", line.unwrap());
        }
    }
    res.wait().unwrap()
}

fn do_layout(file: &mut GraphFile, config: &Config) {
    log!(
        "graph_viewer ssh exited with: {}\r\n",
        run_command(Command::new("ssh").arg("zdimension@domino").arg(format!(
            r"
            cd /home/zdimension/graphrust_tools/GPUGraphLayout/builds/linux;
            rm *.bin;
            ./graph_viewer gpu {} 1 sg 1 1 approximate ../../../edges.txt . bin",
            config.layout_iterations
        )))
    );
    log!(
        "layout.bin scp exited with: {}",
        run_command(Command::new("scp")
            .arg(format!(
                "zdimension@domino:/home/zdimension/graphrust_tools/GPUGraphLayout/builds/linux/edges.txt_{}.bin",
                config.layout_iterations
            ))
            .arg("layout.bin")
            )
    );

    #[derive(Readable)]
    struct GGLNode {
        id: u32,
        x: f32,
        y: f32,
    }
    #[derive(Readable)]
    struct GGLFile {
        #[speedy(length =..)]
        nodes: Vec<GGLNode>,
    }

    for layout_node in GGLFile::read_from_file("layout.bin")
        .unwrap()
        .nodes
        .into_iter()
    {
        file.nodes[layout_node.id as usize].position = Point {
            x: layout_node.x,
            y: layout_node.y,
        };
    }

    log!("Layout done");
}

fn do_modularity(file: &mut GraphFile, config: &Config) {
    log!(
        "gpulouvain ssh exited with: {}",
        run_command(Command::new("ssh").arg("zdimension@domino").arg(format!(
            r"
            cd /home/zdimension/graphrust_tools/gpu-louvain;
            rm *.bin;
            ./gpulouvain -f ../edges.txt -g {}",
            config.community_min_gain
        )))
    );
    log!(
        "comms.bin scp exited with: {}",
        run_command(
            Command::new("scp")
                .arg("zdimension@domino:/home/zdimension/graphrust_tools/gpu-louvain/comms.bin")
                .arg("comms.bin")
        )
    );
    #[derive(Readable)]
    struct GPULouvainFile {
        num_comms: u16,
        #[speedy(length =..)]
        nodes: Vec<u16>,
    }

    let comm_file = GPULouvainFile::read_from_file("comms.bin").unwrap();

    log!("Creating color palette");
    let top_comms = (comm_file.num_comms as f32 * 0.1).ceil() as u16;
    let top_palette = ColorPalette::new(top_comms as u32, PaletteType::Random, false);
    let rest_comms = comm_file.num_comms - top_comms;
    let rest_palette = ColorPalette::new(rest_comms as u32, PaletteType::Random, false);
    let colors = top_palette.colors.iter().chain(rest_palette.colors.iter());

    file.classes.extend(colors.map(|color| Color3b {
        r: (color.red * 255.0) as u8,
        g: (color.green * 255.0) as u8,
        b: (color.blue * 255.0) as u8,
    }));

    log!("Applying modularity classes");
    for (i, comm) in comm_file.nodes.iter().copied().enumerate() {
        file.nodes[i].class = comm;
    }
}

#[tokio::main]
async fn main() {
    *LAST_LOG_TIME.lock().unwrap() = std::time::Instant::now();

    let config: Config = Figment::new()
        .merge(Toml::file("import.toml"))
        .merge(Env::prefixed("IMPORT_"))
        .extract()
        .unwrap();

    log!("Using config: {:#?}", config);

    let n4j_config = ConfigBuilder::default()
        .uri(&config.uri)
        .user(&config.user)
        .password(&config.pass)
        .fetch_size(10485760)
        .build()
        .unwrap();
    log!("Connecting");
    let graph = Graph::connect(n4j_config).await.unwrap();
    log!("Start");
    let mut file = GraphFile::default();
    let mut nodes = graph
        .execute(if config.only_bfs {
            query("match (n) return n.uid, n.name")
        } else {
            query("match (n) where count { (n)--() } >= $mind return n.uid, n.name")
                .param("mind", config.min_degree)
        })
        .await
        .unwrap();
    let mut nodes_ids = AHashMap::new();
    log!("Processing node query");
    while let Ok(Some(row)) = nodes.next().await {
        let uid: String = row.get("n.uid").unwrap();
        let name: String = row
            .get("n.name")
            .expect(format!("Node without name: {}", uid).as_str());
        let pers = NodeStore {
            position: Point { x: 0.0, y: 0.0 },
            size: 0.0,
            class: 0,
            offset_id: file.ids.len() as u32,
            offset_name: file.names.len() as u32,
        };
        nodes_ids.insert(uid.clone(), file.nodes.len());
        file.nodes.push(pers);
        file.ids.extend(uid.as_bytes());
        file.ids.push(0);
        file.names.extend(name.as_bytes());
        file.names.push(0);
    }
    log!("{} nodes", file.nodes.len());

    let mut edges_q = graph
        .execute(
            if config.only_bfs {
                query("match (n)-->(m) return n.uid, m.uid")
            } else {
                query(
                    "match (n)-->(m) where count { (n)--() } >= $mind and count { (m)--() } >= $mind return n.uid, m.uid",
                )
                    .param("mind", config.min_degree)
            },
        )
        .await
        .unwrap();

    let mut edges = Vec::new();
    // write edge list to edges.txt with a buffered writer

    log!("Processing edge query");
    while let Ok(Some(row)) = edges_q.next().await {
        let uid1: String = row.get("n.uid").unwrap();
        let uid2: String = row.get("m.uid").unwrap();
        /*let a = *nodes_ids.get(&uid1).expect(&uid1);
        let b = *nodes_ids.get(&uid2).expect(&uid2);*/
        let Some(&a) = nodes_ids.get(&uid1) else {
            log!("Node not found: {}", uid1);
            continue;
        };
        let Some(&b) = nodes_ids.get(&uid2) else {
            log!("Node not found: {}", uid2);
            continue;
        };
        edges.push((a, b));
        file.edges.push(EdgeStore {
            a: a as u32,
            b: b as u32,
        });
        //writeln!(&mut edges_writer, "{} {}", a, b).unwrap();
    }
    log!("{} edges", edges.len());

    log!("Computing adjacency matrix");
    let adj = file.get_adjacency();

    log!("Running BFS to check if graph contains unconnected nodes");
    let mut covered = vec![false; adj.len()];
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(0);
    covered[0] = true;
    let mut count = 0;
    while let Some(node) = queue.pop_front() {
        count += 1;
        for &neigh in &adj[node as usize] {
            if !covered[neigh as usize] {
                covered[neigh as usize] = true;
                queue.push_back(neigh);
            }
        }
    }
    log!(
        /*count,
        adj.len(),*/
        "Graph contains {} unconnected nodes: {}",
        adj.len() - count,
        covered
            .iter()
            .enumerate()
            .filter(|(_, &c)| !c)
            .map(|(i, _)| i)
            .map(|i| unsafe {
                CStr::from_ptr(file.ids.as_ptr().add(file.nodes[i].offset_id as usize) as *const _)
            }
            .to_str()
            .unwrap())
            .map(|id| format!("bfs('{}', level=1, limit=10)", id))
            .collect::<Vec<_>>()
            .join("\n")
    );

    if config.only_bfs {
        return;
    }

    let edges_file = std::fs::File::create("edges.txt").unwrap();
    let mut edges_writer = std::io::BufWriter::new(&edges_file);
    writeln!(&mut edges_writer, "{} {}", file.nodes.len(), edges.len()).unwrap();
    for EdgeStore { a, b } in file.edges.iter() {
        writeln!(&mut edges_writer, "{} {}", a, b).unwrap();
    }

    log!("Wrote edges file");

    log!(
        "Edges file copied; scp exited with: {}",
        Command::new("scp")
            .arg("edges.txt")
            .arg("zdimension@domino:/home/zdimension/graphrust_tools")
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap()
            .wait()
            .unwrap()
    );

    do_layout(&mut file, &config);

    do_modularity(&mut file, &config);

    log!("Writing metadata");

    file.class_count = file.classes.len() as u16;
    file.node_count = file.nodes.len() as LenType;
    file.edge_count = file.edges.len() as LenType;
    file.ids_size = file.ids.len() as LenType;
    file.names_size = file.names.len() as LenType;

    log!("Writing to file");
    file.write_to_file("graph_n4j.bin").unwrap();

    log!("Compressing file with brotli");

    Command::new("bash")
        .arg("-c")
        .arg("brotli -f -o graph_n4j.bin.br graph_n4j.bin -q 5")
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    log!("Done");
}
