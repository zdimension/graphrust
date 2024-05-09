use crate::app::{Cancelable, iter_progress, ModularityClass, Person, StatusWriter, StringTables, ViewerData};

use graph_format::{EdgeStore, GraphFile, Point};
use itertools::Itertools;
use rayon::prelude::*;

use speedy::Readable;

use crate::utils::{str_from_null_terminated_utf8, SliceExt};

use crate::{for_progress, log, log_progress};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

//const GRAPH_NAME: &str = "graph2.bin";
const GRAPH_NAME: &str = "graph_n4j.bin";
//const GRAPH_NAME: &str = "graph_n4j_5.57M_400k.bin";

#[cfg(not(target_arch = "wasm32"))]
pub fn load_file(_status_tx: &StatusWriter) -> GraphFile {
    GraphFile::read_from_file(format!("{}/../{}", env!("CARGO_MANIFEST_DIR"), GRAPH_NAME)).unwrap()
}

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
    //log!(status_tx, "Edge count: {}", content.edge_count);

    log!(status_tx, "Processing modularity classes");

    let modularity_classes = content
        .classes
        .iter()
        .copied()
        .enumerate()
        .map(|(id, color)| ModularityClass::new(color, id as u16))
        .collect_vec();

    log!(status_tx, "Processing nodes");

    let start = chrono::Local::now();
    let mut person_data: Vec<_> = iter_progress(content.nodes.iter(), &status_tx)
        .map(|node| unsafe {
            Person::new(
                node.position,
                node.size,
                node.class,
                str_from_null_terminated_utf8(content.ids.as_ptr().offset(node.offset_id as isize)),
                str_from_null_terminated_utf8(
                    content.names.as_ptr().offset(node.offset_name as isize),
                ),
                node.total_edge_count as usize,
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
    /*let how_often = (content.edges.len() / 100).max(1);
    for (i, edge) in content.edges.iter().enumerate() {
        if edge.a == edge.b {
            //panic!("Self edge detected"); TODO
            continue;
        }
        let (p1, p2) = person_data.get_two_mut(edge.a as usize, edge.b as usize);
        p1.neighbors.push(edge.b as usize);
        p2.neighbors.push(edge.a as usize);
        if i % how_often == 0 {
            log_progress!(status_tx, i, content.edges.len());
        }
    }*/

    /*for_progress!(status_tx, edge in content.edges.iter(), {
        if edge.a == edge.b {
            //panic!("Self edge detected"); TODO
            continue;
        }
        let (p1, p2) = person_data.get_two_mut(edge.a as usize, edge.b as usize);
        p1.neighbors.push(edge.b as usize);
        p2.neighbors.push(edge.a as usize);
    });*/

    let mut edges = Vec::new();

    for_progress!(status_tx, (i, n) in content.nodes.iter().enumerate(), {
        edges.reserve(n.edge_count as usize);
        for e in n.edges.iter().copied() {
            person_data[i].neighbors.push(e as usize);
            person_data[e as usize].neighbors.push(i);
            edges.push(EdgeStore {
                a: i as u32,
                b: e,
            });
        }
    });

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
        edges: edges,
    })
}
