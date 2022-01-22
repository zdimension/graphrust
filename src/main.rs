mod utils;
mod graph_storage;
mod camera;

use camera::Camera;

use graph_storage::*;

use std::cmp::Ordering;
use std::ffi::CStr;
use std::time::Instant;


use chrono;
use imgui::Ui;

extern crate speedy;

use speedy::{Readable};

#[macro_use]
extern crate glium;
extern crate imgui;
extern crate imgui_glium_renderer;


use itertools::Itertools;
use nalgebra::{Matrix4, Orthographic3, Similarity2, Similarity3, Translation2, UnitQuaternion, Vector2, Vector3};
use winit::dpi::PhysicalPosition;


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
        Vertex { position, color }
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
            color,
            id,
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

struct UiState
{
    g_show_nodes: bool,
    g_show_edges: bool,
infos_current: Option<usize>
}

fn combo_with_filter(ui: &Ui, label: &str, current: &mut Option<usize>)
{

}

fn draw_ui(ui: &mut imgui::Ui, state: &mut UiState, data: &ViewerData)
{
    imgui::Window::new("Graphe")
        .size([400.0, 500.0], imgui::Condition::FirstUseEver)
        .build(ui, ||
            {
                if ui.collapsing_header("Affichage", imgui::TreeNodeFlags::DEFAULT_OPEN)
                {
                    ui.checkbox("Afficher les nœuds", &mut state.g_show_nodes);
                    ui.checkbox("Afficher les liens", &mut state.g_show_edges);
                }

                if ui.collapsing_header("Chemin le plus court", imgui::TreeNodeFlags::DEFAULT_OPEN)
                {}

                if ui.collapsing_header("Informations", imgui::TreeNodeFlags::empty())
                {
                    combo_with_filter(ui, "#infos_user", &mut state.infos_current);
                }
            });
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



    let mut platform = WinitPlatform::init(&mut imgui);
    {
        let gl_window = display.gl_window();
        let window = gl_window.window();

        platform.attach_window(imgui.io_mut(), window, HiDpiMode::Default);
    }

    let font_size = 14.0;

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

    let mut renderer = Renderer::init(&mut imgui, &display).expect("Failed to initialize imgui renderer");

    let vertex_buffer = glium::VertexBuffer::new(&display, &data.vertices).unwrap();
    let indices = glium::index::NoIndices(glium::index::PrimitiveType::TrianglesList);

    let program = glium::Program::from_source(&display, include_str!("graph.vert"), include_str!("graph.frag"), None).unwrap();

    let mut camera = Camera::new(500, 500);

    let mut pressed_left = false;
    let mut pressed_right: Option<f32> = None;
    let mut mouse: PhysicalPosition<f64> = Default::default();

    let mut frames = 0;
    let mut start = std::time::Instant::now();
    let mut last_frame = Instant::now();
    let mut ui_state = UiState {
        g_show_nodes: false,
        g_show_edges: true,
        infos_current: None
    };
    event_loop.run(move |ev, _, control_flow| {
        let next_frame_time = std::time::Instant::now() +
            std::time::Duration::from_nanos(16_666_667);
        *control_flow = glutin::event_loop::ControlFlow::WaitUntil(next_frame_time);

        {
            let gl_window = display.gl_window();
            let window = gl_window.window();
            platform.handle_event(imgui.io_mut(), window, &ev);
        }

        match ev {
            glutin::event::Event::NewEvents(_) =>
                {
                    let now = Instant::now();
                    imgui.io_mut().update_delta_time(now - last_frame);
                    last_frame = now;
                }
            glutin::event::Event::WindowEvent { event, .. } => match event
            {
                glutin::event::WindowEvent::CloseRequested => {
                    *control_flow = glutin::event_loop::ControlFlow::Exit;
                    return;
                }
                glutin::event::WindowEvent::CursorMoved { position, .. } =>
                    {
                        mouse = position;
                    }
                glutin::event::WindowEvent::Resized(size) =>
                    {
                        camera.set_window_size(size.width, size.height);
                    }
                _ => return,
            },
            glutin::event::Event::DeviceEvent { event, .. } => match event
            {
                glutin::event::DeviceEvent::MouseWheel { delta } =>
                    {
                        if imgui.io().want_capture_mouse {
                            return;
                        }
                        let dy = match delta
                        {
                            glutin::event::MouseScrollDelta::LineDelta(_dx, dy) => dy,
                            glutin::event::MouseScrollDelta::PixelDelta(glutin::dpi::PhysicalPosition { y, .. }) => y as f32,
                        };
                        camera.zoom(dy, mouse);
                    }
                glutin::event::DeviceEvent::Button { state, button, .. } =>
                    {
                        match button {
                            1 => {
                                if state == winit::event::ElementState::Pressed && !imgui.io().want_capture_mouse {
                                    pressed_left = true;
                                } else {
                                    pressed_left = false;
                                }
                            }
                            3 => {
                                if state == winit::event::ElementState::Pressed && !imgui.io().want_capture_mouse {
                                    let gl_window = display.gl_window();
                                    let window = gl_window.window();
                                    let inner_size = window.inner_size();
                                    let size_vec2 = Vector2::new(inner_size.width as f32 / 2.0, inner_size.height as f32 / 2.0);
                                    pressed_right = Some((-(mouse.y as f32 - size_vec2.y)).atan2(mouse.x as f32 - size_vec2.x));
                                } else {
                                    pressed_right = None;
                                }
                            }
                            _ => return,
                        }
                    }
                glutin::event::DeviceEvent::MouseMotion { delta } =>
                    {
                        let (dx, dy) = delta;
                        if pressed_left {
                            camera.pan(dx as f32, dy as f32);
                        } else if let Some(vec) = pressed_right {
                            let gl_window = display.gl_window();
                            let window = gl_window.window();
                            let inner_size = window.inner_size();
                            let size_vec2 = Vector2::new(inner_size.width as f32 / 2.0, inner_size.height as f32 / 2.0);
                            let rot = (-(mouse.y as f32 - size_vec2.y)).atan2(mouse.x as f32 - size_vec2.x);
                            camera.rotate(rot - vec);
                            pressed_right = Some(rot);
                        }
                    }
                _ => return,
            },
            glutin::event::Event::MainEventsCleared =>
                {
                    let gl_window = display.gl_window();
                    platform
                        .prepare_frame(imgui.io_mut(), gl_window.window())
                        .expect("Failed to prepare frame");
                    gl_window.window().request_redraw();
                }
            glutin::event::Event::RedrawRequested(_) =>
                {
                    let mut ui = imgui.frame();
                    let gl_window = display.gl_window();
                    let window = gl_window.window();

                    draw_ui(&mut ui, &mut ui_state, &data);

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
                        matrix: *camera.get_matrix().as_ref(),
                    };

                    target.draw(&vertex_buffer, &indices, &program, &uniforms,
                                &Default::default()).unwrap();
                    platform.prepare_render(&ui, gl_window.window());
                    let draw_data = ui.render();
                    renderer
                        .render(&mut target, draw_data)
                        .expect("Rendering failed");
                    target.finish().expect("Failed to swap buffers");
                }
            event =>
                {
                    let gl_window = display.gl_window();
                    let window = gl_window.window();
                    platform.handle_event(imgui.io_mut(), &window, &event);
                }
        }
    });
}
