use imgui_wgpu::{Renderer, RendererConfig};
use instant::Instant;
use nalgebra::Vector2;
use simsearch::SimSearch;
use imgui_winit_support::winit;
use imgui_winit_support::winit::event::{Event, WindowEvent};
use imgui_winit_support::winit::event_loop::ControlFlow;
use winit::dpi::PhysicalPosition;
use pollster::block_on;
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

//implement_vertex!(Vertex, position, color);

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

pub fn main() {
    let data = load_binary();

    log!("Loaded");

    use imgui::{Context, FontConfig, FontGlyphRanges, FontSource};

    let event_loop = winit::event_loop::EventLoop::new();

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..Default::default()
    });

    let builder = winit::window::WindowBuilder::new()
        .with_title("Graphe")
        .with_inner_size(winit::dpi::LogicalSize::new(500f64, 500f64));

    #[cfg(web_platform)]
        let builder = {
        use winit::platform::web::WindowBuilderExtWebSys;
        builder.with_append(true)
    };
    let window = builder.build(&event_loop).unwrap();

    #[cfg(web_platform)]
    wasm::insert_canvas(&window);

    let surface = unsafe { instance.create_surface(&window) }.unwrap();

    let hidpi_factor = window.scale_factor();

    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }))
        .unwrap();

    let (device, queue) =
        block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None)).unwrap();

    let size = window.inner_size();

    // Set up swap chain
    let surface_desc = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Bgra8UnormSrgb,
        width: size.width,
        height: size.height,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![wgpu::TextureFormat::Bgra8Unorm],
    };

    surface.configure(&device, &surface_desc);

    let mut imgui = Context::create();
    let mut platform = imgui_winit_support::WinitPlatform::init(&mut imgui);
    platform.attach_window(
        imgui.io_mut(),
        &window,
        imgui_winit_support::HiDpiMode::Default,
    );
    imgui.set_ini_filename(None);

    let font_size = (13.0 * hidpi_factor) as f32;
    imgui.io_mut().font_global_scale = (1.0 / hidpi_factor) as f32;

    imgui.fonts().add_font(&[FontSource::DefaultFontData {
        config: Some(imgui::FontConfig {
            oversample_h: 1,
            pixel_snap_h: true,
            size_pixels: font_size,
            ..Default::default()
        }),
    }]);

  /*  let mut g = ImFontGlyphRangesBuilder::default();
    let mut ranges = ImVector_ImWchar::default();
    unsafe*/ {
        /*let defrange: [ImWchar; 3] = [0x0020, 0x00FF, 0];
        imgui::sys::ImFontGlyphRangesBuilder_AddRanges(&mut g, defrange.as_ptr());
        imgui::sys::ImFontGlyphRangesBuilder_AddText(&mut g, data.names.as_ptr() as _, data.names.as_ptr().add(data.names.len()) as _);
        imgui::sys::ImFontGlyphRangesBuilder_BuildRanges(&mut g, &mut ranges);*/
/*
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
            }]);*/
    }

    let clear_color = wgpu::Color {
        r: 0.1,
        g: 0.2,
        b: 0.3,
        a: 1.0,
    };

    let renderer_config = RendererConfig {
        texture_format: surface_desc.format,
        ..Default::default()
    };

    let mut renderer = Renderer::new(&mut imgui, &device, &queue, renderer_config);

    /*let vertex_buffer = glium::VertexBuffer::new(&display, &data.vertices).unwrap();
    log!("Vertex buffer size: {}", vertex_buffer.len());
    let indices = glium::index::NoIndices(glium::index::PrimitiveType::TrianglesList);

    let program = glium::Program::from_source(&display, include_str!("shaders/graph.vert"), include_str!("shaders/graph.frag"), None).unwrap();*/

    let mut camera = Camera::new(500, 500);

    let mut pressed_left = false;
    let mut pressed_right: Option<f32> = None;
    let mut mouse: PhysicalPosition<f64> = Default::default();

    let mut frames = 0;
    let mut start = Instant::now();
    let mut last_frame = Instant::now();
    let mut ui_state = UiState::default();
    let mut last_cursor = None;
    event_loop.run(move |ev, _, control_flow| {
        *control_flow = winit::event_loop::ControlFlow::Poll;

        use winit::event::Event::*;
        use winit::event::WindowEvent::*;
        use winit::event::DeviceEvent::*;

        match &ev {
            WindowEvent { event, .. } => match *event
            {
                CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                }
                CursorMoved { position, .. } =>
                    {
                        mouse = position;
                    }
                Resized(size) =>
                    {
                        let surface_desc = wgpu::SurfaceConfiguration {
                            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                            format: wgpu::TextureFormat::Bgra8UnormSrgb,
                            width: size.width,
                            height: size.height,
                            present_mode: wgpu::PresentMode::Fifo,
                            alpha_mode: wgpu::CompositeAlphaMode::Auto,
                            view_formats: vec![wgpu::TextureFormat::Bgra8Unorm],
                        };

                        surface.configure(&device, &surface_desc);
                        camera.set_window_size(size.width, size.height);
                    }
                _ => {},
            },
            DeviceEvent { event, .. } => match *event
            {
                winit::event::DeviceEvent::MouseWheel { delta } =>
                    {
                        if imgui.io().want_capture_mouse {
                            return;
                        }
                        let dy = match delta
                        {
                            winit::event::MouseScrollDelta::LineDelta(_dx, dy) => dy,
                            winit::event::MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition { y, .. }) => y as f32,
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
                            let inner_size = window.inner_size();
                            let size_vec2 = Vector2::new(inner_size.width as f32 / 2.0, inner_size.height as f32 / 2.0);
                            let rot = (-(mouse.y as f32 - size_vec2.y)).atan2(mouse.x as f32 - size_vec2.x);
                            camera.rotate(rot - vec);
                            pressed_right = Some(rot);
                        }
                    }
                _ => {},
            },
            MainEventsCleared => window.request_redraw(),
            RedrawRequested(_) =>
                {
                    let delta_s = last_frame.elapsed();
                    let now = Instant::now();
                    imgui.io_mut().update_delta_time(now - last_frame);
                    last_frame = now;

                    let frame = match surface.get_current_texture() {
                        Ok(frame) => frame,
                        Err(e) => {
                            eprintln!("dropped frame: {e:?}");
                            return;
                        }
                    };
                    platform
                        .prepare_frame(imgui.io_mut(), &window)
                        .expect("Failed to prepare frame");

                    let ui = imgui.frame();
                    ui_state.draw_ui(ui, &data, ());

                    let mut encoder: wgpu::CommandEncoder =
                        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                    if last_cursor != Some(ui.mouse_cursor()) {
                        last_cursor = Some(ui.mouse_cursor());
                        platform.prepare_render(ui, &window);
                    }

                    let view = frame
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(clear_color),
                                store: true,
                            },
                        })],
                        depth_stencil_attachment: None,
                    });

                    renderer
                        .render(imgui.render(), &queue, &device, &mut rpass)
                        .expect("Rendering failed");

                    drop(rpass);

                    queue.submit(Some(encoder.finish()));

                    frame.present();

                    /*let window = gl_window.window();


                    frames += 1;
                    let threshold_ms = 200;
                    if start.elapsed().as_millis() >= threshold_ms {
                        window.set_title(&format!("Graphe - {:.0} fps", frames as f64 / start.elapsed().as_millis() as f64 * 1000.0));
                        start = Instant::now();
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
                    let draw_data = imgui.render();
                    renderer
                        .render(&mut target, draw_data)
                        .expect("Rendering failed");
                    target.finish().expect("Failed to swap buffers");*/
                }
            _ => {}
        }

        platform.handle_event(imgui.io_mut(), &window, &ev);
    });
}

#[cfg(web_platform)]
mod wasm {
    use std::num::NonZeroU32;

    use softbuffer::{Surface, SurfaceExtWeb};
    use wasm_bindgen::prelude::*;
    use winit::{
        event::{Event, WindowEvent},
        window::Window,
    };

    #[wasm_bindgen(start)]
    pub fn run() {
        console_log::init_with_level(log::Level::Debug).expect("error initializing logger");

        #[allow(clippy::main_recursion)]
            let _ = super::main();
    }

    pub fn insert_canvas(window: &Window) {
        use winit::platform::web::WindowExtWebSys;

        let canvas = window.canvas().unwrap();
        let mut surface = Surface::from_canvas(canvas.clone()).unwrap();
        surface
            .resize(
                NonZeroU32::new(canvas.width()).unwrap(),
                NonZeroU32::new(canvas.height()).unwrap(),
            )
            .unwrap();
        let mut buffer = surface.buffer_mut().unwrap();
        buffer.fill(0xFFF0000);
        buffer.present().unwrap();
    }

    pub fn log_event(log_list: &web_sys::Element, event: &Event<()>) {
        log::debug!("{:?}", event);

        // Getting access to browser logs requires a lot of setup on mobile devices.
        // So we implement this basic logging system into the page to give developers an easy alternative.
        // As a bonus its also kind of handy on desktop.
        let event = match event {
            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } => None,
            Event::WindowEvent { event, .. } => Some(format!("{event:?}")),
            Event::Resumed | Event::Suspended => Some(format!("{event:?}")),
            _ => None,
        };
        if let Some(event) = event {
            let window = web_sys::window().unwrap();
            let document = window.document().unwrap();
            let log = document.create_element("li").unwrap();

            let date = js_sys::Date::new_0();
            log.set_text_content(Some(&format!(
                "{:02}:{:02}:{:02}.{:03}: {event}",
                date.get_hours(),
                date.get_minutes(),
                date.get_seconds(),
                date.get_milliseconds(),
            )));

            log_list
                .insert_before(&log, log_list.first_child().as_ref())
                .unwrap();
        }
    }
}