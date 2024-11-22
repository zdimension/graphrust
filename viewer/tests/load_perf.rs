use env_logger;
use itertools::Itertools;
use std::env;
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
    println!("Loadiong");
    let res = load_file(&NullStatusWriter).unwrap();
    println!("Loaded; processing");
    let bin = load_binary(&NullStatusWriter, res).unwrap();

    println!("File processed");

    let viewer = &bin.viewer;

    let get = |name| {
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
    );
}
