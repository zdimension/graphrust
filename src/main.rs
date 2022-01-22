mod utils;


use std::cmp::Ordering;
use std::ffi::CStr;


use chrono;

extern crate speedy;

use speedy::{Readable};

#[macro_use]
extern crate glium;
extern crate imgui;
extern crate imgui_glium_renderer;


use itertools::Itertools;
use nalgebra::{Matrix4, Orthographic3, Similarity2, Similarity3, Translation2, UnitQuaternion, Vector2, Vector3};
use winit::dpi::PhysicalPosition;


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
    fn new(r: f32, g: f32, b: f32) -> Color3f
    {
        Color3f {
            r,
            g,
            b,
        }
    }

    fn average(&self, other: Color3f) -> Color3f
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
struct Point
{
    x: f32,
    y: f32,
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
    neighbors: Vec<(usize, usize)>,
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

#[derive(Copy, Clone)]
struct Vertex
{
    position: Point,
    color: Color3f,
}

implement_vertex!(Vertex, position, color);

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
        //.take(1000)
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

    log!("Done");

    ViewerData {
        persons: person_data,
        vertices: edge_vertices,
        modularity_classes,
        edge_sizes,
    }
}

// 2D orthografic projection
struct Camera
{
    center: Point,
    zoom: f32,
    angle: f32,
}

// use nalgebra
impl Camera
{
    fn get_transformation(&self) -> Similarity3<f32>
    {
        Similarity3::new(
            Vector3::new(-self.center.x, -self.center.y, 0.0),
            self.angle * Vector3::z(),
            self.zoom)
    }

    // return the orthographic projection matrix
    fn get_view_matrix(&self) -> Matrix4<f32>
    {
        self.get_transformation()
            .to_homogeneous()
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
        .with_inner_size(glium::glutin::dpi::LogicalSize::new(500f64, 500f64));
    let cb = glutin::ContextBuilder::new()
        .with_multisampling(4)
        .with_vsync(true)
        ;
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
                rasterizer_multiply: 1.5,
                oversample_h: 4,
                oversample_v: 4,
                ..FontConfig::default()
            }),
        }]);

    let vertex_buffer = glium::VertexBuffer::new(&display, &data.vertices).unwrap();
    let indices = glium::index::NoIndices(glium::index::PrimitiveType::TrianglesList);

    let vertex_shader_src = r#"
        #version 140
        in vec2 position;
        in vec3 color;
        out vec3 my_attr;      // our new attribute
        uniform mat4 matrix;
        uniform mat4 perspective;
        void main() {
            my_attr = color;     // we need to set the value of each `out` variable.
            gl_Position = perspective * matrix * vec4(position, 0.0, 1.0);
        }
    "#;

    let fragment_shader_src = r#"
        #version 140
        in vec3 my_attr;
        out vec4 color;
        void main() {
            color = vec4(my_attr, 1.0);   // we build a vec4 from a vec2 and two floats
        }
    "#;

    let program = glium::Program::from_source(&display, vertex_shader_src, fragment_shader_src, None).unwrap();

    let mut transf: Similarity3<f32> = Similarity3::new(
        Vector3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 0.0),
        1.0);

    let mut ortho = Orthographic3::new(-250.0, 250.0, -250.0, 250.0, -10.0, 10.0);

    let mut pressed_left = false;
    let mut pressed_right = false;
    let mut mouse: PhysicalPosition<f64> = Default::default();

    let mut frames = 0;
    let mut start = std::time::Instant::now();
    event_loop.run(move |ev, _, control_flow| {
        let next_frame_time = std::time::Instant::now() +
            std::time::Duration::from_nanos(16_666_667);
        *control_flow = glutin::event_loop::ControlFlow::WaitUntil(next_frame_time);

        let gl_window = display.gl_window();
        let window = gl_window.window();
        let inner_size = window.inner_size();
        let size_vec2 = Vector2::new(inner_size.width as f32 / 2.0, inner_size.height as f32 / 2.0);

        match ev {
            glutin::event::Event::WindowEvent { event, .. } => match event {
                glutin::event::WindowEvent::CloseRequested => {
                    *control_flow = glutin::event_loop::ControlFlow::Exit;
                    return;
                }
                glutin::event::WindowEvent::CursorMoved { position, .. } =>
                    {
                        mouse = position;
                    }
                glutin::event::WindowEvent::Resized(size) => {
                    let w = size.width as f32 / 2.0;
                    let h = size.height as f32 / 2.0;
                    ortho.set_left_and_right(-w, w);
                    ortho.set_bottom_and_top(-h, h);
                }
                _ => return,
            },
            glutin::event::Event::DeviceEvent { event, .. } => match event {
                /*glutin::event::DeviceEvent::MouseMotion { delta } => {
                let (dx, dy) = delta;
                camera.angle += dx as f32 / 100.0;
                camera.center.x += dy as f32 / 100.0;
            }*/
                glutin::event::DeviceEvent::MouseWheel { delta } => {
                    let dy = match delta
                    {
                        glutin::event::MouseScrollDelta::LineDelta(_dx, dy) => dy,
                        glutin::event::MouseScrollDelta::PixelDelta(glutin::dpi::PhysicalPosition { y, .. }) => y as f32,
                    };
                    let zoom_speed = 1.1;
                    let s = if dy > 0.0 { zoom_speed } else { 1.0 / zoom_speed };
                    let mouse_vec2 = Vector2::new(mouse.x as f32, mouse.y as f32);
                    let diff = mouse_vec2 - size_vec2;
                    let diffpoint = nalgebra::Point3::new(diff.x, diff.y, 0.0);
                    let before = transf.inverse_transform_point(&diffpoint);
                    transf.append_scaling_mut(s);
                    let after = transf.inverse_transform_point(&diffpoint);
                    transf.append_translation_mut(&nalgebra::Translation3::new((after.x - before.x) * transf.scaling(), -(after.y - before.y) * transf.scaling(), 0.0));
                }
                glutin::event::DeviceEvent::Button { state, button, .. } => {
                    match button {
                        1 => {
                            if state == winit::event::ElementState::Pressed {
                                pressed_left = true;
                            } else {
                                pressed_left = false;
                            }
                        }
                        3 => {
                            if state == winit::event::ElementState::Pressed {
                                pressed_right = true;
                            } else {
                                pressed_right = false;
                            }
                        }
                        _ => return,
                    }
                }
                glutin::event::DeviceEvent::MouseMotion { delta } => {
                    let (dx, dy) = delta;
                    if pressed_left {
                        transf.append_translation_mut(&nalgebra::Translation3::new(dx as f32, -dy as f32, 0.0));
                    }
                    /*else if pressed_right {
                    let mouse_vec2 = Vector2::new(mouse.x as f32, mouse.y as f32);
                    let diff = mouse_vec2 - size_vec2;
                    let diffpoint = nalgebra::Point3::new(diff.x, diff.y, 0.0);
                    let rot = diff.y.atan2(diff.x);
                    let center = transf.inverse_transform_point(&diffpoint);
                    transf.append_rotation_wrt_point_mut(&nalgebra::UnitQuaternion::from_euler_angles(0.0, 0.0, rot), &center);
                }*/
                }
                _ => return,
            },
            glutin::event::Event::MainEventsCleared => {
                frames += 1;
                let threshold_ms = 200;
                if start.elapsed().as_millis() >= threshold_ms {
                    window.set_title(&format!("Graphe - {:.0} fps", frames as f64 / start.elapsed().as_millis() as f64 * 1000.0));
                    start = std::time::Instant::now();
                    frames = 0;
                }

                let mut target = display.draw();

                target.clear_color(1.0, 1.0, 1.0, 1.0);

                let uniforms = uniform! {
                    matrix: *transf.to_homogeneous().as_ref(),
                    perspective: *ortho.to_homogeneous().as_ref(),
                };

                target.draw(&vertex_buffer, &indices, &program, &uniforms,
                            &Default::default()).unwrap();
                target.finish().unwrap();
            }
            _ => return,
        }
    });
}
