#![warn(clippy::all, rust_2018_idioms)]
#![feature(cmp_minmax)]
#![feature(try_blocks)]
#![cfg_attr(target_arch = "wasm32", feature(stdarch_wasm_atomic_wait))]
//#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

// When compiling natively:
#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    use std::{env, io};
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info");
    }
    env::set_var("RUST_LOG", "debug");
    env_logger::builder()
        .format(|buf, record| {
            use io::Write;
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

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1600.0, 900.0]),
        renderer: eframe::Renderer::Glow,
        multisampling: 4,
        ..Default::default()
    };
    eframe::run_native(
        "eframe template",
        native_options,
        Box::new(|cc| Ok(Box::new(viewer::GraphViewApp::new(cc)))),
    )
}

#[cfg(target_arch = "wasm32")]
use eframe::web_sys;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;


// When compiling to web using trunk:
#[cfg(target_arch = "wasm32")]
fn main() {
    use log::*;

    #[cfg(target_arch = "wasm32")]
    use eframe::WebLogger;

    WebLogger::init(LevelFilter::Debug).expect("Failed to initialize WebLogger");

    log::info!("Setting panic hook");
    use std::panic;
    panic::set_hook(Box::new(console_error_panic_hook::hook));

    log::info!("Start called {}", chrono::Local::now().format("%H:%M:%S.%3f"));
    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        eframe::WebRunner::new()
            .start(
                "the_canvas_id", // hardcode it
                web_options,
                Box::new(|cc| Box::new(viewer::GraphViewApp::new(cc))),
            )
            .await
            .expect("failed to start eframe");
    });
}
