#![feature(cmp_minmax)]

use graph_format::{Color3b, EdgeStore, GraphFile, LenType, NodeStore, Point, Readable, Writable};
use speedy::*;
use std::collections::{HashMap, HashSet};
use std::ffi::CStr;
use std::process::Command;

#[derive(Readable, Writable)]
pub struct NodeStore2 {
    pub position: Point,
    pub size: f32,
    pub class: u16,
    pub offset_id: u32,
    pub offset_name: u32,
    pub total_edge_count: u16,
    pub edge_count: u16,
    #[speedy(length = edge_count)]
    pub edges: Vec<u32>,
}

#[derive(Readable, Default)]
#[cfg_attr(target_pointer_width = "64", derive(Writable))]
pub struct GraphFile2 {
    pub class_count: u16,
    #[speedy(length = class_count)]
    pub classes: Vec<Color3b>,

    pub node_count: LenType,
    #[speedy(length = node_count)]
    pub nodes: Vec<NodeStore2>,

    pub ids_size: LenType,
    #[speedy(length = ids_size)]
    pub ids: Vec<u8>,

    pub names_size: LenType,
    #[speedy(length = names_size)]
    pub names: Vec<u8>,
}

struct UniqueCounter {
    val: HashMap<u32, u32>,
}

impl FromIterator<u32> for UniqueCounter {
    fn from_iter<I: IntoIterator<Item=u32>>(iter: I) -> Self {
        let mut val = HashMap::new();
        for i in iter {
            *val.entry(i).or_insert(0) += 1;
        }
        UniqueCounter { val }
    }
}

impl UniqueCounter {
    fn len(&self) -> i32 {
        self.val.len() as i32
    }

    fn remove_one(&mut self, key: u32) {
        let count = self.val.get_mut(&key).unwrap();
        *count -= 1;
        if *count == 0 {
            self.val.remove(&key);
        }
    }

    fn add_one(&mut self, key: u32) {
        *self.val.entry(key).or_insert(0) += 1;
    }
}

pub unsafe fn str_from_null_terminated_utf8<'a>(s: *const u8) -> &'a str {
    CStr::from_ptr(s as *const _).to_str().unwrap()
}

fn main() {
    let f = GraphFile::read_from_file("graph_n4j.bin").unwrap();

    const LIMIT: usize = 10000;

    let mut new_graph = Vec::new();
    let adj = f.get_adjacency();

    let mut edges = HashSet::new();

    for (node_id, neighbors) in adj[..LIMIT].into_iter().enumerate() {
        // let mut new_neighbors = Vec::new();
        // for neighbor in neighbors {
        //     if (*neighbor as usize) < LIMIT {
        //         new_neighbors.push(*neighbor);
        //     }
        // }
        //
        let new_neighbors: Vec<_> = neighbors.into_iter().filter(|n| **n < LIMIT as u32).map(|n| *n).collect();

        edges.extend(new_neighbors.iter().map(|nb| {
            let [a, b] = std::cmp::minmax(node_id, *nb as usize);
            (a, b)
        }));

        new_graph.push(new_neighbors);
    }

    let adj = new_graph;

    use std::io::Write;
    let edges_file = std::fs::File::create(r"Z:\home\zdimension\graphrust_tools\Graph-Betweenness-Centrality\csr.txt").unwrap();
    let mut edges_writer = std::io::BufWriter::new(&edges_file);
    writeln!(&mut edges_writer, "{} {}", adj.len(), edges.len()).unwrap();
    println!("{} {} {}", adj.len(), edges.len(), adj.len() * edges.len());

    // cumsum of adj len
    /*let mut cumsum = 0;
    loop {
        write!(&mut edges_writer, "{} ", cumsum).unwrap();

    }*/
    println!("Writing counts");
    write!(&mut edges_writer, "0").unwrap();
    let mut cum = 0;
    for list in &adj {
        cum += list.len();
        write!(&mut edges_writer, " {}", cum).unwrap();
    }
    writeln!(&mut edges_writer).unwrap();

    println!("Writing edges");
    for list in adj {
        for e in list {
            write!(&mut edges_writer, "{} ", e).unwrap();
        }
    }

    /*let names = f.nodes.iter().map(|p| {
        unsafe {
            (
                str_from_null_terminated_utf8(
                    f.ids.as_ptr().offset(p.offset_id as isize),
                ),
                str_from_null_terminated_utf8(
                    f.names.as_ptr().offset(p.offset_name as isize),
                ))
        }
    }).filter(|s| s.1.len() > 255);

    for name in names {
        println!("{:?}", name);
    }*/

    //println!("max name length: {}", names.unwrap());

    /*let mut edges: Vec<EdgeStore> = f.edges;

    let mut unique_a = edges.iter().map(|e| e.a).collect::<UniqueCounter>();
    let mut unique_b = edges.iter().map(|e| e.b).collect::<UniqueCounter>();

    println!("initial: {} {}", unique_a.len(), unique_b.len());

    if unique_a.len() > unique_b.len() {
        for e in edges.iter_mut() {
            (e.a, e.b) = (e.b, e.a);
        }
    }

    edges.sort_unstable_by_key(|e| (e.a, e.b));

    let mut f2 = GraphFile2 {
        class_count: f.class_count,
        classes: f.classes,
        node_count: f.node_count,
        nodes: f
            .nodes
            .iter()
            .map(|n| NodeStore2 {
                position: n.position,
                size: n.size,
                class: n.class,
                offset_id: n.offset_id,
                offset_name: n.offset_name,
                total_edge_count: 0,
                edge_count: 0,
                edges: vec![],
            })
            .collect(),
        ids_size: f.ids_size,
        ids: f.ids,
        names_size: f.names_size,
        names: f.names,
    };

    /*for (i, edge) in edges.iter_mut().enumerate() {
        if i % 2 == 0 {
            (edge.a, edge.b) = (edge.b, edge.a);
        }
    }*/

    /*let mut last_delta = 0; // we want to maximize this
    let mut any_changed;
    let mut iterations = 0;
    loop {
        any_changed = false;

        for i in 0..edges.len() {
            let elem = &mut edges[i];

            unique_a.remove_one(elem.a);
            unique_b.remove_one(elem.b);
            unique_a.add_one(elem.b);
            unique_b.add_one(elem.a);

            (elem.a, elem.b) = (elem.b, elem.a);

            let new_delta = (unique_a.len() - unique_b.len()).abs();

            if new_delta > last_delta {
                last_delta = new_delta;
                any_changed = true;
            } else if new_delta < last_delta {
                let elem = &mut edges[i];

                unique_a.remove_one(elem.a);
                unique_b.remove_one(elem.b);
                unique_a.add_one(elem.b);
                unique_b.add_one(elem.a);

                (elem.a, elem.b) = (elem.b, elem.a);
            }
        }

        iterations += 1;

        if iterations % 1 == 0 {
            println!(
                "{} {} ({} iterations)",
                unique_a.len(),
                unique_b.len(),
                iterations
            );
        }

        if !any_changed {
            break;
        }
    }

    println!(
        "final: {} {} ({} iterations)",
        unique_a.len(),
        unique_b.len(),
        iterations
    );*/

    for e in edges {
        let node_b = &mut f2.nodes[e.b as usize];
        node_b.edges.push(e.a);
        node_b.total_edge_count += 1;
        f2.nodes[e.a as usize].total_edge_count += 1;

        //f2.nodes[e.a as usize].edges.push(e.b);
    }

    for n in f2.nodes.iter_mut() {
        n.edges.sort();
        n.edge_count = n.edges.len() as u16;
    }

    f2.write_to_file("graph_n4j_0805.bin").unwrap();

    Command::new("bash")
        .arg("-c")
        .arg("brotli -f -o graph_n4j_0805.bin.br graph_n4j_0805.bin -q 5")
        .spawn()
        .unwrap()
        .wait()
        .unwrap();*/
}
