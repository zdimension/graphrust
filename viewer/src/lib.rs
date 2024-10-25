#![warn(clippy::all, rust_2018_idioms)]
#![feature(cmp_minmax)]
#![feature(try_blocks)]
#![feature(specialization)]
#![feature(map_many_mut)]
#![allow(incomplete_features)]

mod app;
mod camera;
mod combo_filter;
mod geom_draw;
pub mod graph_storage;
mod ui;
pub mod utils;
mod algorithms;

pub use app::thread;
pub use app::GraphViewApp;
