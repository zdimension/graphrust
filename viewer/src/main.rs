#![warn(clippy::all, rust_2018_idioms)]
#![feature(cmp_minmax)]
//#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

// When compiling natively:
#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
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
        Box::new(|cc| Box::new(viewer::GraphViewApp::new(cc))),
    )
}

// When compiling to web using trunk:
#[cfg(target_arch = "wasm32")]
fn main() {
    use log::*;

    #[cfg(target_arch = "wasm32")]
    use eframe::WebLogger;

    WebLogger::init(LevelFilter::Debug).expect("Failed to initialize WebLogger");
}

#[cfg(target_arch = "wasm32")]
use eframe::web_sys;
use log::{LevelFilter, Log};
use std::{env, io};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn start() {
    log::info!("Start called");
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
