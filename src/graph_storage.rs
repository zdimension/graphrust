use itertools::Itertools;
use nalgebra::Vector2;
use simsearch::SimSearch;
use speedy::{Readable};
use crate::{log, ModularityClass, Person, Vertex, ViewerData};
use crate::utils::{SliceExt, str_from_null_terminated_utf8};

// 24bpp color structure
#[derive(Copy, Clone)]
#[derive(Readable)]
pub struct Color3b
{
    r: u8,
    g: u8,
    b: u8,
}

// same but f32
#[derive(Copy, Clone)]
pub struct Color3f
{
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color3b
{
    pub fn to_f32(&self) -> Color3f
    {
        Color3f {
            r: self.r as f32 / 255.0,
            g: self.g as f32 / 255.0,
            b: self.b as f32 / 255.0,
        }
    }
}

impl Color3f
{
    fn new(r: f32, g: f32, b: f32) -> Color3f
    {
        Color3f {
            r,
            g,
            b,
        }
    }

    pub fn average(&self, other: Color3f) -> Color3f
    {
        Color3f {
            r: (self.r + other.r) / 2.0,
            g: (self.g + other.g) / 2.0,
            b: (self.b + other.b) / 2.0,
        }
    }
}

unsafe impl glium::vertex::Attribute for Color3f
{
    fn get_type() -> glium::vertex::AttributeType
    {
        glium::vertex::AttributeType::F32F32F32
    }
}


#[derive(Copy, Clone)]
#[derive(Readable)]
pub struct Point
{
    pub x: f32,
    pub y: f32,
}

impl Into<Vector2<f32>> for Point
{
    fn into(self) -> Vector2<f32>
    {
        Vector2::new(self.x, self.y)
    }
}

impl Into<Point> for Vector2<f32>
{
    fn into(self) -> Point
    {
        Point {
            x: self.x,
            y: self.y,
        }
    }
}

unsafe impl glium::vertex::Attribute for Point
{
    fn get_type() -> glium::vertex::AttributeType
    {
        glium::vertex::AttributeType::F32F32
    }
}

impl Point
{
    pub fn new(x: f32, y: f32) -> Point
    {
        Point { x, y }
    }

    pub fn polar(r: f32, theta: f32) -> Point
    {
        Point { x: r * theta.cos(), y: r * theta.sin() }
    }

    pub fn norm(&self) -> f32
    {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn ortho(&self) -> Point
    {
        Point { x: -self.y, y: self.x }
    }

    pub fn normalized(&self) -> Point
    {
        *self / self.norm()
    }
}

impl std::ops::Add for Point
{
    type Output = Point;

    fn add(self, other: Point) -> Point
    {
        Point { x: self.x + other.x, y: self.y + other.y }
    }
}

impl std::ops::Sub for Point
{
    type Output = Point;

    fn sub(self, other: Point) -> Point
    {
        Point { x: self.x - other.x, y: self.y - other.y }
    }
}

impl std::ops::Mul<f32> for Point
{
    type Output = Point;

    fn mul(self, other: f32) -> Point
    {
        Point { x: self.x * other, y: self.y * other }
    }
}

impl std::ops::Div<f32> for Point
{
    type Output = Point;

    fn div(self, other: f32) -> Point
    {
        Point { x: self.x / other, y: self.y / other }
    }
}


#[derive(Readable)]
pub struct NodeStore
{
    pub position: Point,
    pub size: f32,
    pub class: u16,
    pub offset_id: u32,
    pub offset_name: u32,
}

#[derive(Readable)]
pub struct EdgeStore
{
    pub a: u32,
    pub b: u32,
}

#[derive(Readable)]
pub struct GraphFile
{
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

pub fn load_binary<'a>() -> ViewerData<'a>
{
    log!("Loading binary");
    let content: GraphFile = GraphFile::read_from_file("graph2.bin").unwrap();
    log!("Binary content loaded");
    log!("Class count: {}", content.class_count);
    log!("Node count: {}", content.node_count);
    log!("Edge count: {}", content.edge_count);

    log!("Processing modularity classes");

    let modularity_classes = content.classes
        .iter().enumerate()
        .map(|(id, color)| ModularityClass::new(color.to_f32(), id as u16))
        .collect_vec();

    struct VertexInter
    {
        a: (u32, Point),
        b: (u32, Point),
        dist: f32,
        color: Color3f,
    }

    log!("Processing edges");

    let mut edge_data = content.edges
        .iter()
        .map(|edge|
            {
                let a = &content.nodes[edge.a as usize];
                let b = &content.nodes[edge.b as usize];
                let dist = (a.position - b.position).norm();
                let color = modularity_classes[a.class as usize].color.average(modularity_classes[b.class as usize].color);
                VertexInter { a: (edge.a, a.position), b: (edge.b, b.position), dist, color }
            })
        .collect_vec();

    log!("Sorting edges");
    edge_data.sort_by(|a, b| b.dist.partial_cmp(&a.dist).unwrap());

    log!("Drawing edges");

    let edge_vertices = edge_data.iter()
        .flat_map(|edge|
            {
                let ortho = (edge.b.1 - edge.a.1).ortho().normalized();
                let v0 = edge.a.1 + ortho;
                let v1 = edge.a.1 - ortho;
                let v2 = edge.b.1 - ortho;
                let v3 = edge.b.1 + ortho;
                let color = edge.color;
                vec![
                    Vertex::new(v0, color),
                    Vertex::new(v1, color),
                    Vertex::new(v2, color),
                    Vertex::new(v2, color),
                    Vertex::new(v3, color),
                    Vertex::new(v0, color),
                ]
            })
        .collect_vec();

    let edge_sizes = edge_data.iter().map(|edge| edge.dist).collect_vec();

    log!("Processing nodes");

    let mut person_data = content.nodes.iter()
        .map(|node|
            unsafe {
                Person::new(node.position, node.size, node.class as u16,
                            str_from_null_terminated_utf8(content.ids.as_ptr().offset(node.offset_id as isize)),
                            str_from_null_terminated_utf8(content.names.as_ptr().offset(node.offset_name as isize)),
                )
            }
        )
        .collect_vec();

    log!("Generating neighbor lists");

    for (i, edge) in edge_data.iter().enumerate()
    {
        let (p1, p2) = person_data.get_two_mut(edge.a.0 as usize, edge.b.0 as usize);
        p1.neighbors.push((edge.a.0 as usize, i));
        p2.neighbors.push((edge.b.0 as usize, i));
    }

    log!("Creating search engine");
    let mut engine: SimSearch<usize> = SimSearch::new();
    for (i, person) in person_data.iter().enumerate()
    {
        engine.insert(i, &person.name);
    }

    log!("Done");

    ViewerData {
        ids: content.ids,
        names: content.names,
        persons: person_data,
        vertices: edge_vertices,
        modularity_classes,
        edge_sizes,
        engine
    }
}