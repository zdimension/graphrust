use ahash::AHashMap;
use derivative::Derivative;
use figment::providers::{Env, Format, Toml};
use figment::Figment;
use forceatlas2::{Layout, Nodes, Settings};
use graph_format::{Color3b, EdgeStore, GraphFile, LenType, NodeStore, Point};
use neo4rs::{query, ConfigBuilder, Graph};
use serde::Deserialize;
use speedy::Writable;
use std::sync::Mutex;

#[derive(Deserialize, Derivative)]
#[derivative(Default)]
#[serde(default)]
struct Config {
    #[derivative(Default(value = "\"127.0.0.1:7687\".to_string()"))]
    uri: String,
    #[derivative(Default(value = "\"neo4j\".to_string()"))]
    user: String,
    #[derivative(Default(value = "\"password\".to_string()"))]
    pass: String,
    #[derivative(Default(value = "5"))]
    min_degree: u32,
    #[derivative(Default(value = "100"))]
    layout_iterations: usize,
    #[derivative(Default(value = "8"))]
    threads: usize,
    #[derivative(Default(value = "1024"))]
    chunk_size: usize,
}

static LAST_LOG_TIME: Mutex<std::time::Instant> =
    Mutex::new(unsafe { std::mem::transmute([0u8; std::mem::size_of::<std::time::Instant>()]) });

macro_rules! log
{
    ($($arg:tt)*) =>
    {
        {
            let mut last_log_time = LAST_LOG_TIME.lock().unwrap();
            let now = std::time::Instant::now();
            let elapsed = now - *last_log_time;
            *last_log_time = now;
            println!("[{}] [{:>5}ms] [{}:{}] {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S.%3f"),
                elapsed.as_millis(),
                file!(), line!(), format_args!($($arg)*));
        }
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

    let n4j_config = ConfigBuilder::default()
        .uri(config.uri)
        .user(config.user)
        .password(config.pass)
        .fetch_size(1048576)
        .build()
        .unwrap();
    log!("Connecting");
    let graph = Graph::connect(n4j_config).await.unwrap();
    log!("Start");
    let mut file = GraphFile::default();
    let mut nodes = graph
        .execute(
            query("match (n) where count { (n)--() } > $mind return n.uid, n.name")
                .param("mind", config.min_degree),
        )
        .await
        .unwrap();
    let mut nodes_ids = AHashMap::new();
    log!("Processing node query");
    while let Ok(Some(row)) = nodes.next().await {
        let uid: String = row.get("n.uid").unwrap();
        let name: String = row.get("n.name").unwrap();
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
        .execute(query(
            "match (n)-->(m) where count { (n)--() } > $mind and count { (m)--() } > $mind return n.uid, m.uid",
        ).param("mind", config.min_degree))
        .await
        .unwrap();

    let mut edges = Vec::new();
    log!("Processing edge query");
    while let Ok(Some(row)) = edges_q.next().await {
        let uid1: String = row.get("n.uid").unwrap();
        let uid2: String = row.get("m.uid").unwrap();
        let a = *nodes_ids.get(&uid1).expect(&uid1);
        let b = *nodes_ids.get(&uid2).expect(&uid2);
        edges.push((a, b));
        file.edges.push(EdgeStore {
            a: a as u32,
            b: b as u32,
        });
    }
    log!("{} edges", edges.len());

    rayon::ThreadPoolBuilder::new()
        .num_threads(config.threads)
        .build_global()
        .ok();

    let mut layout = Layout::<f32>::from_graph(
        edges,
        Nodes::Degree(file.nodes.len()),
        None,
        None,
        Settings {
            //barnes_hut: Some(1.2),
            barnes_hut: None,
            chunk_size: Some(config.chunk_size),
            dimensions: 2,
            dissuade_hubs: false,
            ka: 0.01,
            kg: 0.001,
            kr: 0.002,
            lin_log: false,
            speed: 1.0,
            prevent_overlapping: None,
            strong_gravity: false,
        },
    );

    const IT_COUNT: usize = 100;
    for i in 0..IT_COUNT {
        layout.iteration();
        if i % (IT_COUNT / 10) == 0 {
            log!("Iteration {}", i);
        }
    }

    log!("Fetching positins");
    for (i, p) in layout.points.iter().enumerate() {
        file.nodes[i].position = Point { x: p[0], y: p[1] };
    }

    log!("Writing metadata");
    file.classes.push(Color3b { r: 255, g: 0, b: 0 });

    file.class_count = file.classes.len() as u16;
    file.node_count = file.nodes.len() as LenType;
    file.edge_count = file.edges.len() as LenType;
    file.ids_size = file.ids.len() as LenType;
    file.names_size = file.names.len() as LenType;

    log!("Writing to file");
    file.write_to_file("graph_n4j.bin").unwrap();

    log!("Done");
}
