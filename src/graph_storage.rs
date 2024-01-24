use crate::app::{ModularityClass, Person, ViewerData};
use crate::geom_draw::create_rectangle;
use crate::log;
use itertools::Itertools;
use nalgebra::Vector2;
use simsearch::SimSearch;
use speedy::Readable;

use crate::utils::{str_from_null_terminated_utf8, SliceExt};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

// 24bpp color structure
#[derive(Copy, Clone, Readable)]
pub struct Color3b {
    r: u8,
    g: u8,
    b: u8,
}

// same but f32
#[derive(Copy, Clone)]
pub struct Color3f {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color3b {
    pub fn to_f32(self) -> Color3f {
        Color3f {
            r: self.r as f32 / 255.0,
            g: self.g as f32 / 255.0,
            b: self.b as f32 / 255.0,
        }
    }
}

impl Color3f {
    pub fn new(r: f32, g: f32, b: f32) -> Color3f {
        Color3f { r, g, b }
    }

    pub fn average(&self, other: Color3f) -> Color3f {
        Color3f {
            r: (self.r + other.r) / 2.0,
            g: (self.g + other.g) / 2.0,
            b: (self.b + other.b) / 2.0,
        }
    }
}
/*
unsafe impl glium::vertex::Attribute for Color3f
{
    fn get_type() -> glium::vertex::AttributeType
    {
        glium::vertex::AttributeType::F32F32F32
    }
}*/

/// 2D point/vector.
#[derive(Copy, Clone, Readable)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl From<Point> for Vector2<f32> {
    fn from(p: Point) -> Vector2<f32> {
        Vector2::new(p.x, p.y)
    }
}

impl From<Vector2<f32>> for Point {
    fn from(v: Vector2<f32>) -> Point {
        Point { x: v.x, y: v.y }
    }
}
/*
unsafe impl glium::vertex::Attribute for Point
{
    fn get_type() -> glium::vertex::AttributeType
    {
        glium::vertex::AttributeType::F32F32
    }
}
*/
impl Point {
    pub fn new(x: f32, y: f32) -> Point {
        Point { x, y }
    }

    /// Returns the unit vector with angle theta.
    pub fn polar(theta: f32) -> Point {
        Point {
            x: theta.cos(),
            y: theta.sin(),
        }
    }

    /// Returns the distance between the point and the origin.
    pub fn norm(&self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    /// Returns the canonical orthogonal vector.
    pub fn ortho(&self) -> Point {
        Point {
            x: -self.y,
            y: self.x,
        }
    }

    /// Normalizes the vector.
    pub fn normalized(&self) -> Point {
        *self / self.norm()
    }
}

impl std::ops::Add for Point {
    type Output = Point;

    fn add(self, other: Point) -> Point {
        Point {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl std::ops::Sub for Point {
    type Output = Point;

    fn sub(self, other: Point) -> Point {
        Point {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl std::ops::Mul<f32> for Point {
    type Output = Point;

    fn mul(self, other: f32) -> Point {
        Point {
            x: self.x * other,
            y: self.y * other,
        }
    }
}

impl std::ops::Div<f32> for Point {
    type Output = Point;

    fn div(self, other: f32) -> Point {
        Point {
            x: self.x / other,
            y: self.y / other,
        }
    }
}

#[derive(Readable)]
pub struct NodeStore {
    pub position: Point,
    pub size: f32,
    pub class: u16,
    pub offset_id: u32,
    pub offset_name: u32,
}

#[derive(Readable)]
pub struct EdgeStore {
    pub a: u32,
    pub b: u32,
}

#[derive(Readable)]
pub struct GraphFile {
    pub class_count: u16,
    #[speedy(length = class_count)]
    pub classes: Vec<Color3b>,

    pub node_count: u64,
    #[speedy(length = node_count)]
    pub nodes: Vec<NodeStore>,

    pub edge_count: u64,
    #[speedy(length = edge_count)]
    pub edges: Vec<EdgeStore>,

    pub ids_size: u64,
    #[speedy(length = ids_size)]
    pub ids: Vec<u8>,

    pub names_size: u64,
    #[speedy(length = names_size)]
    pub names: Vec<u8>,
}

const GRAPH_NAME: &str = "graph2.bin";

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

pub struct ProcessedData<'a> {
    pub viewer: ViewerData<'a>,
    pub edges: Vec<EdgeStore>,
}

pub fn load_binary<'a>() -> ProcessedData<'a> {
    log!("Loading binary");
    let content: GraphFile = load_file();
    log!("Binary content loaded");
    log!("Class count: {}", content.class_count);
    log!("Node count: {}", content.node_count);
    log!("Edge count: {}", content.edge_count);

    log!("Processing modularity classes");

    let modularity_classes = content
        .classes
        .iter()
        .enumerate()
        .map(|(id, color)| ModularityClass::new(color.to_f32(), id as u16))
        .collect_vec();

    log!("Processing nodes");

    let mut person_data = content
        .nodes
        .iter()
        .map(|node| unsafe {
            Person::new(
                node.position,
                node.size,
                node.class as u16,
                str_from_null_terminated_utf8(content.ids.as_ptr().offset(node.offset_id as isize)),
                str_from_null_terminated_utf8(
                    content.names.as_ptr().offset(node.offset_name as isize),
                ),
            )
        })
        .collect_vec();

    log!("Generating neighbor lists");

    for (i, edge) in content.edges.iter().enumerate() {
        if edge.a == edge.b {
            //panic!("Self edge detected"); TODO
            continue;
        }
        let (p1, p2) = person_data.get_two_mut(edge.a as usize, edge.b as usize);
        p1.neighbors.push((edge.b as usize, i));
        p2.neighbors.push((edge.a as usize, i));
    }

    log!("Initializing search engine");
    let mut engine: SimSearch<usize> = SimSearch::new();
    for (i, person) in person_data.iter().enumerate() {
        engine.insert(i, person.name);
    }

    log!("Done");

    ProcessedData {
        viewer: ViewerData {
            ids: content.ids,
            names: content.names,
            persons: person_data,
            modularity_classes,
            engine,
        },
        edges: content.edges,
    }
}
