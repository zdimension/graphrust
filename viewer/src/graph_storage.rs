use crate::app::{Cancelable, ModularityClass, Person, StatusWriter, StringTables, ViewerData};

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
pub fn load_file(_status_tx: &StatusWriter) -> GraphFile {
    GraphFile::read_from_file(format!("{}/../{}", env!("CARGO_MANIFEST_DIR"), GRAPH_NAME)).unwrap()
}
/*
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch, method, structural, js_class = "XMLHttpRequest", js_name = open)]
    pub fn open(
        this: &web_sys::XmlHttpRequest,
        method: &str,
        url: &str,
        async_: bool,
    ) -> Result<(), JsValue>;
}
*/
/*
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(inline_js = "export function req(url) { const xhr = new XMLHttpRequest(); xhr.open('GET', url, false); xhr.responseType = 'arraybuffer'; xhr.send(); console.log(xhr.response.byteLength); return new Uint8Array(xhr.response); }")]
extern "C" {
    fn req(url: &str) -> js_sys::Uint8Array;
}*/

#[cfg(target_arch = "wasm32")]
pub fn load_file(_status_tx: &StatusWriter) -> GraphFile {
    let url = "https://domino.zdimension.fr/web/network5/graph_n4j.bin.br";
    let xhr = web_sys::XmlHttpRequest::new().unwrap();
    xhr.open_with_async("GET", url, false).unwrap();
    xhr.set_response_type(web_sys::XmlHttpRequestResponseType::Arraybuffer);
    xhr.send().unwrap();
    let array_buffer = xhr.response().unwrap();
    let vec = js_sys::Uint8Array::new(&array_buffer).to_vec();
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

pub fn load_binary(status_tx: StatusWriter) -> Cancelable<ProcessedData> {
    log!(status_tx, "Loading binary");
    let content: GraphFile = load_file(&status_tx);
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

    Ok(ProcessedData {
        strings: StringTables {
            ids: content.ids,
            names: content.names,
        },
        viewer: ViewerData::new(person_data, modularity_classes, &status_tx)?,
        edges: content.edges,
    })
}
