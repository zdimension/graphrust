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

    #[cfg(feature = "deadlock_detection")]
    { // only for #[cfg]
        use std::thread;
        use std::time::Duration;
        use parking_lot::deadlock;

        // Create a background thread which checks for deadlocks every 10s
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(10));
                let deadlocks = deadlock::check_deadlock();
                if deadlocks.is_empty() {
                    continue;
                }

                println!("{} deadlocks detected", deadlocks.len());
                for (i, threads) in deadlocks.iter().enumerate() {
                    println!("Deadlock #{}", i);
                    for t in threads {
                        println!("Thread Id {:#?}", t.thread_id());
                        println!("{:#?}", t.backtrace());
                    }
                }
            }
        });
    } // only for #[cfg]

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

#[cfg(target_arch = "wasm32")]
pub use wasm_bindgen_rayon::init_thread_pool;

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
                web_sys::window().unwrap().document().unwrap().get_element_by_id("the_canvas_id").unwrap().dyn_into().unwrap(),
                web_options,
                Box::new(|cc| Ok(Box::new(viewer::GraphViewApp::new(cc)))),
            )
            .await
            .expect("failed to start eframe");
    });
}
