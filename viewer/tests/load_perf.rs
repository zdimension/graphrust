use env_logger;
use std::env;
use viewer::graph_storage::load_binary;

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
    println!("Current directory: {:?}", env::current_dir().unwrap());
    let _ = load_binary();
}
