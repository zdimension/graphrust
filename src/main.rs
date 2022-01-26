#[macro_use]
extern crate glium;
extern crate imgui;
extern crate imgui_glium_renderer;
extern crate speedy;

use std::time::Instant;
use nalgebra::Vector2;
use simsearch::SimSearch;
use winit::dpi::PhysicalPosition;

use camera::Camera;
use graph_storage::*;

use crate::combo_filter::combo_with_filter;
use crate::geom_draw::{create_circle_tris, create_rectangle};
use crate::ui::UiState;

mod utils;
mod graph_storage;
mod camera;
mod combo_filter;
mod geom_draw;
mod ui;

const FONT_SIZE: f32 = 14.0;

pub struct Person<'a>
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
            position,
            size,
            modularity_class,
            id,
            name,
            sorted_id: 0,
            neighbors: Vec::new(),
        }
    }
}

#[derive(Copy, Clone)]
pub struct Vertex
{
    pub position: Point,
    pub color: Color3f,
}

implement_vertex!(Vertex, position, color);

impl Vertex
{
    fn new(position: Point, color: Color3f) -> Vertex
    {
        Vertex { position, color }
    }
}

pub struct ModularityClass<'a>
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

pub struct ViewerData<'a>
{
    pub ids: Vec<u8>,
    pub names: Vec<u8>,
    pub persons: Vec<Person<'a>>,
    pub vertices: Vec<Vertex>,
    pub modularity_classes: Vec<ModularityClass<'a>>,
    pub edge_sizes: Vec<f32>,
    pub engine: SimSearch<usize>,
}

fn main() {
    let data = load_binary();

    log!("Loaded");

    use glium::{Surface};

    use imgui::{Context, FontConfig, FontGlyphRanges, FontSource};
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

  /*  let mut g = ImFontGlyphRangesBuilder::default();
    let mut ranges = ImVector_ImWchar::default();
    unsafe*/ {
        /*let defrange: [ImWchar; 3] = [0x0020, 0x00FF, 0];
        imgui::sys::ImFontGlyphRangesBuilder_AddRanges(&mut g, defrange.as_ptr());
        imgui::sys::ImFontGlyphRangesBuilder_AddText(&mut g, data.names.as_ptr() as _, data.names.as_ptr().add(data.names.len()) as _);
        imgui::sys::ImFontGlyphRangesBuilder_BuildRanges(&mut g, &mut ranges);*/

        imgui.fonts().add_font(&[
            FontSource::TtfData {
                data: include_bytes!("../Roboto-Medium.ttf"),
                size_pixels: FONT_SIZE,
                config: Some(FontConfig {
                    rasterizer_multiply: 1.5,
                    oversample_h: 4,
                    oversample_v: 4,
                    // glyph_ranges: FontGlyphRanges::from_ptr(ranges.Data as _),
                    glyph_ranges: FontGlyphRanges::from_slice(&[0x0020, 0xFFFF, 0]),
                    ..FontConfig::default()
                }),
            }]);
    }

    let mut renderer = Renderer::init(&mut imgui, &display).expect("Failed to initialize imgui renderer");

    let vertex_buffer = glium::VertexBuffer::new(&display, &data.vertices).unwrap();
    let indices = glium::index::NoIndices(glium::index::PrimitiveType::TrianglesList);

    let program = glium::Program::from_source(&display, include_str!("shaders/graph.vert"), include_str!("shaders/graph.frag"), None).unwrap();

    let mut camera = Camera::new(500, 500);

    let mut pressed_left = false;
    let mut pressed_right: Option<f32> = None;
    let mut mouse: PhysicalPosition<f64> = Default::default();

    let mut frames = 0;
    let mut start = std::time::Instant::now();
    let mut last_frame = Instant::now();
    let mut ui_state = UiState::default();
    event_loop.run(move |ev, _, control_flow| {
        let next_frame_time = std::time::Instant::now() +
            std::time::Duration::from_nanos(16_666_667);
        *control_flow = glutin::event_loop::ControlFlow::WaitUntil(next_frame_time);

        {
            let gl_window = display.gl_window();
            let window = gl_window.window();
            platform.handle_event(imgui.io_mut(), window, &ev);
        }

        use glutin::event::Event::*;
        use glutin::event::WindowEvent::*;
        use glutin::event::DeviceEvent::*;

        match ev {
            NewEvents(_) =>
                {
                    let now = Instant::now();
                    imgui.io_mut().update_delta_time(now - last_frame);
                    last_frame = now;
                }
            WindowEvent { event, .. } => match event
            {
                CloseRequested => {
                    *control_flow = glutin::event_loop::ControlFlow::Exit;
                }
                CursorMoved { position, .. } =>
                    {
                        mouse = position;
                    }
                Resized(size) =>
                    {
                        camera.set_window_size(size.width, size.height);
                    }
                _ => {},
            },
            DeviceEvent { event, .. } => match event
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
                Button { state, button, .. } =>
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
                            _ => {},
                        }
                    }
                MouseMotion { delta } =>
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
                _ => {},
            },
            MainEventsCleared =>
                {
                    let gl_window = display.gl_window();
                    platform
                        .prepare_frame(imgui.io_mut(), gl_window.window())
                        .expect("Failed to prepare frame");
                    gl_window.window().request_redraw();
                }
            RedrawRequested(_) =>
                {
                    let mut ui = imgui.frame();
                    let gl_window = display.gl_window();
                    let window = gl_window.window();

                    ui_state.draw_ui(&mut ui, &data, &display);

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

                    if let Some(ref vbuf) = ui_state.path_vbuf
                    {
                        target.draw(vbuf, &indices, &program, &uniforms,
                                    &Default::default()).unwrap();
                    }

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
                    platform.handle_event(imgui.io_mut(), window, &event);
                }
        }
    });
}
