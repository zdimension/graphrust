use crate::app::{ModularityClass, Person, StringTables, ViewerData};

use graph_format::{EdgeStore, GraphFile, Point};
use itertools::Itertools;
use nalgebra::Vector2;
use simsearch::SimSearch;
use speedy::{Readable, Writable};

use crate::utils::{str_from_null_terminated_utf8, SliceExt};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

//const GRAPH_NAME: &str = "graph2.bin";
const GRAPH_NAME: &str = "graph_n4j.bin";

#[cfg(not(target_arch = "wasm32"))]
pub fn load_file() -> GraphFile {
    GraphFile::read_from_file(GRAPH_NAME).unwrap()
}

#[cfg(target_arch = "wasm32")]
pub fn load_file() -> GraphFile {
    let wnd = eframe::web_sys::window().unwrap();
    let resp = wnd.get("graph");
    if let Some(val) = resp {
        if !val.is_undefined() {
            let u8a = js_sys::Uint8Array::new(&val);
            let bytes = u8a.to_vec();
            return GraphFile::read_from_buffer(bytes.as_slice()).unwrap();
        }
    }
    panic!("Cannot load graph file");
}

pub struct ProcessedData<'graph> {
    pub strings: StringTables,
    pub viewer: ViewerData<'graph>,
    pub edges: Vec<EdgeStore>,
}

pub fn load_binary<'graph>() -> ProcessedData<'graph> {
    log::info!("Loading binary");
    let content: GraphFile = load_file();
    log::info!("Binary content loaded");
    log::info!("Class count: {}", content.class_count);
    log::info!("Node count: {}", content.node_count);
    log::info!("Edge count: {}", content.edge_count);

    log::info!("Processing modularity classes");

    let modularity_classes = content
        .classes
        .iter()
        .enumerate()
        .map(|(id, color)| ModularityClass::new(color.to_f32(), id as u16))
        .collect_vec();

    log::info!("Processing nodes");

    let mut person_data = content
        .nodes
        .iter()
        .map(|node| unsafe {
            Person::new(
                node.position + Point::new(0.0, 3000.0),
                node.size,
                node.class as u16,
                str_from_null_terminated_utf8(content.ids.as_ptr().offset(node.offset_id as isize)),
                str_from_null_terminated_utf8(
                    content.names.as_ptr().offset(node.offset_name as isize),
                ),
            )
        })
        .collect_vec();

    log::info!("Generating neighbor lists");

    for (i, edge) in content.edges.iter().enumerate() {
        if edge.a == edge.b {
            //panic!("Self edge detected"); TODO
            continue;
        }
        let (p1, p2) = person_data.get_two_mut(edge.a as usize, edge.b as usize);
        p1.neighbors.push(edge.b as usize);
        p2.neighbors.push(edge.a as usize);
    }

    log::info!("Done");

    ProcessedData {
        strings: StringTables {
            ids: content.ids,
            names: content.names,
        },
        viewer: ViewerData::new(person_data, modularity_classes),
        edges: content.edges,
    }
}
