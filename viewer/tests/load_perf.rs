use ahash::HashSet;
use env_logger;
use itertools::Itertools;
use rand::Rng;
use std::env;
use std::num::NonZeroU16;
use viewer::algorithms::pathfinding::{do_pathfinding, PathSectionSettings};
use viewer::graph_storage::{load_binary, load_file};
use viewer::threading::NullStatusWriter;

#[test]
fn init_logs() {
    env::set_var("RUST_LOG", "debug");
    env_logger::builder()
        .format(|buf, record| {
            use std::io::Write;
            writeln!(
                buf,
                "[{}] [{}:{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S.%3f"),
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.args()
            )
        })
        .init();
}

#[test]
fn it_works() {
    // print the current directory
    //println!("Current directory: {:?}", env::current_dir().unwrap());
    log::info!("Loading");
    let res = load_file(&NullStatusWriter).unwrap();
    log::info!("Loaded; processing");
    let bin = load_binary(&NullStatusWriter, res).unwrap();

    log::info!("File processed");

    let viewer = &bin.viewer;
    let rng = &mut rand::thread_rng();

    let mut node = rng.gen_range(0..viewer.persons.len());
    let mut found_already = HashSet::default();
    for _ in 0..10 {
        // find furthest node using bfs
        let mut dist = vec![0; viewer.persons.len()];
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(node);
        dist[node] = 1;
        while let Some(cur) = queue.pop_front() {
            for &neigh in &viewer.persons[cur].neighbors {
                if dist[neigh] == 0 {
                    dist[neigh] = dist[cur] + 1;
                    queue.push_back(neigh);
                }
            }
        }
        /*let max_dist = dist.iter().max().unwrap();
        let furthest = dist
            .iter()
            .enumerate()
            .find(|(_, &d)| d == *max_dist && !found_already.contains(&d))
            .unwrap()
            .0;*/
        let furthest = dist
            .iter()
            .enumerate()
            .filter(|(i, _)| !found_already.contains(i))
            .max_by_key(|(_, &d)| d)
            .unwrap()
            .0;
        found_already.insert(furthest);
        let path = do_pathfinding(
            PathSectionSettings {
                path_src: Some(node),
                path_dest: Some(furthest),
                exclude_ids: vec![],
                path_no_direct: false,
                path_no_mutual: false,
            },
            &viewer.persons,
        )
        .unwrap()
        .path;
        log::info!(
            "diam = {} ({}); path [{}] : [{}]",
            dist[furthest],
            furthest,
            path.len(),
            path.iter()
                .map(|i| viewer.persons[*i].neighbors.len().to_string())
                .join(", ")
        );
        node = furthest;
    }

    /*for _ in 0..1000 {
        let node1 = rng.gen_range(0..viewer.persons.len());
        let node2 = rng.gen_range(0..viewer.persons.len());

        let path = do_pathfinding(
            PathSectionSettings {
                path_src: Some(node1),
                path_dest: Some(node2),
                exclude_ids: vec![],
                path_no_direct: false,
                path_no_mutual: false,
            },
            &viewer.persons,
        )
        .unwrap();

        let path2 = do_pathfinding(
            PathSectionSettings {
                path_src: Some(node1),
                path_dest: Some(node2),
                exclude_ids: vec![],
                path_no_direct: false,
                path_no_mutual: false,
            },
            &viewer.persons,
        )
        .unwrap();

        assert_eq!(path.path, path2.path);
    }*/

    /* let get = |name| {
        let r = viewer
            .engine
            .get_blocking(|s| s.search(name, 1)[0] as usize);
        println!("{}: {:?}", name, r);
        r
    };

    let swann = get("Benziane Swann");

    let craby = get("Craby Craby");

    let blaibiron = get("Charli BlaBiron");

    let etienne = get("Etienne Marais");

    let tom = get("Tom Niget");

    let path = do_pathfinding(
        PathSectionSettings {
            path_src: Some(swann),
            path_dest: Some(etienne),
            exclude_ids: vec![tom],
            path_no_direct: false,
            path_no_mutual: false,
        },
        &viewer.persons,
    )
    .unwrap();

    println!(
        "{:?}",
        path.path
            .iter()
            .map(|&id| &viewer.persons[id].name)
            .collect_vec()
    );*/
}
