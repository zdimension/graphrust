use crate::app::{ModularityClass, Person, StatusWriter, StringTables, ViewerData};

use graph_format::{EdgeStore, GraphFile, Point};
use itertools::Itertools;
use rayon::prelude::*;

use speedy::Readable;

use crate::utils::{str_from_null_terminated_utf8, SliceExt};

use crate::log;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

//const GRAPH_NAME: &str = "graph2.bin";
const GRAPH_NAME: &str = "graph_n4j.bin";

#[cfg(not(target_arch = "wasm32"))]
pub fn load_file() -> GraphFile {
    GraphFile::read_from_file(format!("{}/../{}", env!("CARGO_MANIFEST_DIR"), GRAPH_NAME)).unwrap()
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn download_file() -> JsValue {
    use eframe::web_sys::{Request, RequestInit, RequestMode, Response};
    use wasm_bindgen_futures::JsFuture;
    let url = "https://domino.zdimension.fr/web/network5/graph_n4j.bin.br";
    let window = js_sys::global()
        .dyn_into::<eframe::web_sys::WorkerGlobalScope>()
        .unwrap();
    let resp_value = JsFuture::from(window.fetch_with_str(url)).await.unwrap();
    let resp: Response = resp_value.dyn_into().unwrap();
    let buffer = JsFuture::from(resp.array_buffer().unwrap()).await.unwrap();
    buffer
}

#[cfg(target_arch = "wasm32")]

pub fn load_file() -> GraphFile {
    let buffer = download_file();
    let u8a = js_sys::Uint8Array::new(&buffer);
    let vec = u8a.to_vec();
    return GraphFile::read_from_buffer(&vec).unwrap();

    /*use std::sync::{Arc, Mutex};
    let request = ehttp::Request::get("https://domino.zdimension.fr/web/network5/graph_n4j.bin.br");
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let bytes_clone = bytes.clone();
    ehttp::streaming::fetch(
        request,
        move |result: ehttp::Result<ehttp::streaming::Part>| {
            let part = match result {
                Ok(part) => part,
                Err(err) => {
                    log::error!("Failed to fetch graph file: {:?}", err);
                    return std::ops::ControlFlow::Break(());
                }
            };
            match part {
                ehttp::streaming::Part::Response(response) => {
                    log::info!("Status code: {:?}", response.status);
                    if response.ok {
                        std::ops::ControlFlow::Continue(())
                    } else {
                        std::ops::ControlFlow::Break(())
                    }
                }
                ehttp::streaming::Part::Chunk(chunk) => {
                    let mut bytes = bytes_clone.lock().unwrap();
                    bytes.extend_from_slice(&chunk);
                    std::ops::ControlFlow::Continue(())
                }
            }
        },
    );
    return GraphFile::read_from_buffer(bytes.lock().unwrap().as_slice()).unwrap();*/
}

pub struct ProcessedData {
    pub strings: StringTables,
    pub viewer: ViewerData,
    pub edges: Vec<EdgeStore>,
}

pub fn load_binary(status_tx: StatusWriter) -> ProcessedData {
    log!(status_tx, "Loading binary");
    let content: GraphFile = load_file();
    log!(status_tx, "Binary content loaded");
    log!(status_tx, "Class count: {}", content.class_count);
    log!(status_tx, "Node count: {}", content.node_count);
    log!(status_tx, "Edge count: {}", content.edge_count);

    log!(status_tx, "Processing modularity classes");

    let modularity_classes = content
        .classes
        .iter()
        .enumerate()
        .map(|(id, color)| ModularityClass::new(color.to_f32(), id as u16))
        .collect_vec();

    log!(status_tx, "Processing nodes");

    let start = chrono::Local::now();
    let mut person_data: Vec<_> = content
        .nodes
        .par_iter()
        .map(|node| unsafe {
            Person::new(
                node.position,
                node.size,
                node.class,
                str_from_null_terminated_utf8(content.ids.as_ptr().offset(node.offset_id as isize)),
                str_from_null_terminated_utf8(
                    content.names.as_ptr().offset(node.offset_name as isize),
                ),
            )
        })
        .collect();

    log!(
        status_tx,
        "Done, took {}ms",
        (chrono::Local::now() - start).num_milliseconds()
    );

    log!(status_tx, "Generating neighbor lists");

    let start = chrono::Local::now();
    for (_i, edge) in content.edges.iter().enumerate() {
        if edge.a == edge.b {
            //panic!("Self edge detected"); TODO
            continue;
        }
        let (p1, p2) = person_data.get_two_mut(edge.a as usize, edge.b as usize);
        p1.neighbors.push(edge.b as usize);
        p2.neighbors.push(edge.a as usize);
    }

    log!(
        status_tx,
        "Done, took {}ms",
        (chrono::Local::now() - start).num_milliseconds()
    );

    ProcessedData {
        strings: StringTables {
            ids: content.ids,
            names: content.names,
        },
        viewer: ViewerData::new(person_data, modularity_classes, &status_tx),
        edges: content.edges,
    }
}
