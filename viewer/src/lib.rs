#![warn(clippy::all, rust_2018_idioms)]
#![feature(cmp_minmax)]
#![feature(try_blocks)]
#![feature(specialization)]
#![feature(map_many_mut)]
#![feature(negative_impls)]
#![feature(auto_traits)]
#![feature(box_patterns)]
#![allow(incomplete_features)]
#[macro_use]
extern crate rust_i18n;

i18n!("locales", 
    fallback = "en",
    minify_key = true,
    minify_key_len = 12,
    minify_key_prefix = "tr_",
    minify_key_thresh = 8);
mod app;
pub mod graph_storage;
mod ui;
pub mod utils;
mod algorithms;
mod threading;
mod graph_render;
mod gfonts;
mod http;
mod search;

pub use app::thread;
pub use app::GraphViewApp;
