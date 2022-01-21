mod utils;

use utils::*;

use std::cmp::Ordering;
use std::ffi::CStr;
use std::fs::File;

use glfw::{Context, SwapInterval};
use chrono;

extern crate speedy;

use speedy::{Readable};

#[macro_use]
extern crate glium;
#[macro_use]
extern crate imgui;
extern crate imgui_glium_renderer;

extern crate binread;

use binread::*;
use glium::glutin::dpi::LogicalSize;
use glium::glutin::dpi::Size::Logical;
use itertools::Itertools;


// 24bpp color structure
#[derive(Copy, Clone)]
#[derive(Readable)]
struct Color3b
{
    r: u8,
    g: u8,
    b: u8,
}

// same but f32
#[derive(Copy, Clone)]
struct Color3f
{
    r: f32,
    g: f32,
    b: f32,
}

impl Color3b
{
    fn to_f32(&self) -> Color3f
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
    fn average(&self, other: Color3f) -> Color3f
    {
        Color3f {
            r: (self.r + other.r) / 2.0,
            g: (self.g + other.g) / 2.0,
            b: (self.b + other.b) / 2.0,
        }
    }
}

#[derive(Copy, Clone)]
#[derive(Readable)]
struct Point
{
    x: f32,
    y: f32,
}

impl Point
{
    fn new(x: f32, y: f32) -> Point
    {
        Point { x: x, y: y }
    }

    fn polar(r: f32, theta: f32) -> Point
    {
        Point { x: r * theta.cos(), y: r * theta.sin() }
    }

    fn norm(&self) -> f32
    {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    fn ortho(&self) -> Point
    {
        Point { x: -self.y, y: self.x }
    }

    fn normalized(&self) -> Point
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
struct NodeStore
{
    position: Point,
    size: f32,
    class: u16,
    offset_id: u32,
    offset_name: u32,
}

#[derive(Readable)]
struct EdgeStore
{
    a: u32,
    b: u32,
}

#[derive(Readable)]
struct GraphFile
{
    class_count: u16,
    #[speedy(length = class_count)]
    classes: Vec<Color3b>,

    node_count: u64,
    #[speedy(length = node_count)]
    nodes: Vec<NodeStore>,

    edge_count: u64,
    #[speedy(length = edge_count)]
    edges: Vec<EdgeStore>,

    ids_size: u64,
    #[speedy(length = ids_size)]
    ids: Vec<u8>,

    names_size: u64,
    #[speedy(length = names_size)]
    names: Vec<u8>,
}

struct Person<'a>
{
    position: Point,
    size: f32,
    modularity_class: u16,
    id: &'a str,
    name: &'a str,
    sorted_id: u64,
    neighbors: Vec<(&'a Person<'a>, usize)>,
}

impl<'a> Person<'a>
{
    fn new(position: Point, size: f32, modularity_class: u16, id: &'a str, name: &'a str) -> Person<'a>
    {
        Person {
            position: position,
            size: size,
            modularity_class: modularity_class,
            id: id,
            name: name,
            sorted_id: 0,
            neighbors: Vec::new(),
        }
    }
}

struct Vertex
{
    position: Point,
    color: Color3f,
}

impl Vertex
{
    fn new(position: Point, color: Color3f) -> Vertex
    {
        Vertex { position: position, color: color }
    }
}

struct ModularityClass<'a>
{
    color: Color3f,
    id: u16,
    name: String,
    people: Option<Vec<&'a Person<'a>>>,
}

impl<'a> ModularityClass<'a>
{
    fn new(color: Color3f, id: u16) -> ModularityClass<'a>
    {
        ModularityClass {
            color: color,
            id: id,
            name: format!("Classe {}", id),
            people: None,
        }
    }

    fn get_people(&mut self, data: &'a ViewerData<'a>) -> &Vec<&'a Person<'a>>
    {
        match self.people
        {
            Some(ref people) => people,
            None =>
                {
                    let filtered = data.persons.iter().filter(|p| p.modularity_class == self.id).collect();
                    self.people = Some(filtered);
                    self.people.as_ref().unwrap()
                }
        }
    }
}

unsafe fn str_from_null_terminated_utf8<'a>(s: *const u8) -> &'a str {
    CStr::from_ptr(s as *const _).to_str().unwrap()
}

pub trait SliceExt {
    type Item;

    fn get_two_mut(&mut self, index0: usize, index1: usize) -> (&mut Self::Item, &mut Self::Item);
}

impl<T> SliceExt for [T] {
    type Item = T;

    fn get_two_mut(&mut self, index0: usize, index1: usize) -> (&mut Self::Item, &mut Self::Item) {
        match index0.cmp(&index1) {
            Ordering::Less => {
                let mut iter = self.iter_mut();
                let item0 = iter.nth(index0).unwrap();
                let item1 = iter.nth(index1 - index0 - 1).unwrap();
                (item0, item1)
            }
            Ordering::Equal => panic!("[T]::get_two_mut(): received same index twice ({})", index0),
            Ordering::Greater => {
                let mut iter = self.iter_mut();
                let item1 = iter.nth(index1).unwrap();
                let item0 = iter.nth(index0 - index1 - 1).unwrap();
                (item0, item1)
            }
        }
    }
}

struct ViewerData<'a>
{
    persons: Vec<Person<'a>>,
    vertices: Vec<Vertex>,
    modularity_classes: Vec<ModularityClass<'a>>,
    edge_sizes: Vec<f32>,
}

fn load_binary<'a>() -> ViewerData<'a>
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
                let v2 = edge.b.1 + ortho;
                let v3 = edge.b.1 - ortho;
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

    /*for (i, edge) in edge_data.iter().enumerate()
    {
        let (p1, p2) = person_data.get_two_mut(edge.a.0 as usize, edge.b.0 as usize);
        p1.neighbors.push((p2, i));
        p2.neighbors.push((p1, i));
    }*/

    let (p1, p2) = person_data.get_two_mut(0, 21);
    p1.neighbors.push((p2, 0));
    p2.neighbors.push((p1, 0));

    log!("Done");

    ViewerData {
        persons: person_data,
        vertices: edge_vertices,
        modularity_classes,
        edge_sizes,
    }
}


fn main() {
    let data = load_binary();

    log!("Loaded");

    use glium::{glutin, Surface};

    use imgui::{Context, FontConfig, FontGlyphRanges, FontSource, Ui};
    use imgui_glium_renderer::Renderer;
    use imgui_winit_support::{HiDpiMode, WinitPlatform};

    let event_loop = glutin::event_loop::EventLoop::new();
    let wb = glutin::window::WindowBuilder::new()
        .with_title("Graphe")
        .with_inner_size(glium::glutin::dpi::LogicalSize::new(512f64, 512f64));
    let cb = glutin::ContextBuilder::new();
    let display = glium::Display::new(wb, cb, &event_loop).expect("Failed to initialize display");

    let mut imgui = Context::create();
    imgui.set_ini_filename(None);

    /*let mut platform = WinitPlatform::init(&mut imgui);
    {
        let gl_window = display.gl_window();
        let window = gl_window.window();

        platform.attach_window(imgui.io_mut(), window, HiDpiMode::Default);
    }*/

    let font_size = 13.0;

    imgui.fonts().add_font(&[
        FontSource::TtfData {
            data: include_bytes!("../Roboto-Medium.ttf"),
            size_pixels: font_size,
            config: Some(FontConfig {
                // As imgui-glium-renderer isn't gamma-correct with
                // it's font rendering, we apply an arbitrary
                // multiplier to make the font a bit "heavier". With
                // default imgui-glow-renderer this is unnecessary.
                rasterizer_multiply: 1.5,
                // Oversampling font helps improve text rendering at
                // expense of larger font atlas texture.
                oversample_h: 4,
                oversample_v: 4,
                ..FontConfig::default()
            }),
        }]);

    event_loop.run(move |ev, _, control_flow| {
        let mut target = display.draw();
        target.clear_color(0.0, 0.0, 1.0, 1.0);
        target.finish().unwrap();

        let next_frame_time = std::time::Instant::now() +
            std::time::Duration::from_nanos(16_666_667);

        *control_flow = glutin::event_loop::ControlFlow::WaitUntil(next_frame_time);
        match ev {
            glutin::event::Event::WindowEvent { event, .. } => match event {
                glutin::event::WindowEvent::CloseRequested => {
                    *control_flow = glutin::event_loop::ControlFlow::Exit;
                    return;
                }
                _ => return,
            },
            _ => (),
        }
    });

    /*let mut glfw = glfw::init(glfw::FAIL_ON_ERRORS).unwrap();

    let (mut window, _events) = glfw
        .create_window(500, 500, "Graph", glfw::WindowMode::Windowed)
        .expect("Failed to create GLFW window.");

    window.make_current();
    glfw.set_swap_interval(SwapInterval::Sync(1));

    gl::load_with(|s| window.get_proc_address(s) as *const _);*/

    /**/
}
