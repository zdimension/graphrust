mod utils;
mod graph_storage;
mod camera;
mod combo_filter;

use std::collections::{HashSet, VecDeque};
use std::ffi::CString;
use std::ops::Add;
use camera::Camera;
use graph_storage::*;
use std::time::Instant;
use chrono;
use imgui::sys::{ImFontGlyphRangesBuilder, ImGuiDir, ImGuiHoveredFlags_None, ImU32, ImVec2, ImVector_ImWchar, ImWchar};
use imgui::*;

use nalgebra::Vector2;

extern crate speedy;

use speedy::{Readable};
use simsearch::SimSearch;

#[macro_use]
extern crate glium;
extern crate imgui;
extern crate imgui_glium_renderer;

use winit::dpi::PhysicalPosition;
use crate::combo_filter::combo_with_filter;

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

use derivative::*;

#[derive(Derivative)]
#[derivative(Default)]
struct UiState
{
    g_show_nodes: bool,
    #[derivative(Default(value = "true"))]
    g_show_edges: bool,
    infos_current: Option<usize>,
    infos_open: bool,
    path_src: Option<usize>,
    path_dest: Option<usize>,
    found_path: Option<Vec<usize>>,
    exclude_ids: Vec<usize>,
    path_dirty: bool,
    path_no_direct: bool,
    path_no_mutual: bool,
    path_status: String,
}

use array_tool::vec::Intersect;
use itertools::Itertools;

impl UiState
{
    fn set_infos_current(&mut self, id: Option<usize>)
    {
        self.infos_current = id;
        self.infos_open = id.is_some();
    }

    fn do_pathfinding(&mut self, data: &ViewerData)
    {
        let src_id = self.path_src.unwrap();
        let dest_id = self.path_dest.unwrap();
        let src = &data.persons[src_id];
        let dest = &data.persons[dest_id];

        let intersect = if self.path_no_mutual
        {
            let src_friends = src.neighbors.iter().map(|&(i, _)| i).collect_vec();
            let dest_friends = dest.neighbors.iter().map(|&(i, _)| i).collect_vec();
            HashSet::from_iter(src_friends.intersect(dest_friends))
        }
        else
        {
            HashSet::new()
        };

        let exclude_set: HashSet<usize> = HashSet::from_iter(self.exclude_ids.iter().cloned());

        let mut queue = VecDeque::new();
        let mut visited = vec![false; data.persons.len()];
        let mut pred = vec![None; data.persons.len()];
        let mut dist = vec![i32::MAX; data.persons.len()];

        visited[src_id] = true;
        dist[src_id] = 0;
        queue.push_back(src_id);



        while let Some(id) = queue.pop_front()
        {
            let person = &data.persons[id];
            for &(i, _) in person.neighbors.iter()
            {
                if self.path_no_direct && id == src_id && i == dest_id
                {
                    continue;
                }

                if self.path_no_mutual && intersect.contains(&i)
                {
                    continue;
                }

                if exclude_set.contains(&i)
                {
                    continue;
                }

                if !visited[i]
                {
                    visited[i] = true;
                    dist[i] = dist[id] + 1;
                    pred[i] = Some(id);
                    queue.push_back(i);

                    if i == dest_id
                    {
                        let mut path = Vec::new();

                        path.push(dest_id);

                        let mut cur = dest_id;
                        while let Some(p) = pred[cur]
                        {
                            path.push(p);
                            cur = p;
                        }

                        self.found_path = Some(path);

                        return;
                    }
                }
            }
        }

        self.found_path = None;
    }
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
                {
                    let c1 = combo_with_filter(ui, "#path_src", &mut state.path_src, &data);
                    if c1
                    {
                        state.set_infos_current(state.path_src);
                    }
                    ui.same_line();
                    if ui.button("x##src")
                    {
                        state.path_src = None;
                        state.found_path = None;
                    }

                    let c2 = combo_with_filter(ui, "#path_dest", &mut state.path_dest, &data);
                    if c2
                    {
                        state.set_infos_current(state.path_dest);
                    }
                    ui.same_line();
                    if ui.button("x##dest")
                    {
                        state.path_dest = None;
                        state.found_path = None;
                    }

                    let exw = ui.calc_item_width();
                    ui.set_next_item_width(exw);
                    ui.text("Exclure :");
                    ui.same_line();
                    if ui.button("x##exclall")
                    {
                        state.exclude_ids.clear();
                    }

                    {
                        let mut cur_excl = None;
                        let mut del_excl = None;
                        for (i, id) in state.exclude_ids.iter().enumerate()
                        {
                            if ui.button_with_size(format!("{}##exclbtn", data.persons[*id].name), [exw, 0.0])
                            {
                                cur_excl = Some(*id);
                            }
                            ui.same_line();
                            if ui.button(format!("x##excl{}", i))
                            {
                                del_excl = Some(i);
                            }
                        }
                        if let Some(id) = cur_excl
                        {
                            state.set_infos_current(Some(id));
                        }
                        if let Some(i) = del_excl
                        {
                            state.path_dirty = true;
                            state.exclude_ids.remove(i);
                        }
                    }

                    if (state.path_dirty || c1 || c2)
                        | ui.checkbox("Éviter chemin direct", &mut state.path_no_direct)
                        | ui.checkbox("Éviter amis communs", &mut state.path_no_mutual)
                    {
                        state.path_dirty = false;
                        state.path_status = match (state.path_src, state.path_dest)
                        {
                            (Some(x), Some(y)) if x == y => String::from("Source et destination sont identiques"),
                            (None, _) | (_, None) => String::from(""),
                            _ =>
                                {
                                    state.do_pathfinding(&data);
                                    match state.found_path
                                    {
                                        Some(ref path) => format!("Chemin trouvé, longueur {}", path.len()),
                                        None => String::from("Aucun chemin trouvé"),
                                    }
                            }
                        }
                    }

                    ui.text(state.path_status.as_str());

                    let mut del_path = None;
                    let mut cur_path = None;
                    if let Some(ref path) = state.found_path
                    {
                        for (i, id) in path.iter().enumerate()
                        {
                            if ui.button_with_size(format!("{}##pathbtn", data.persons[*id].name), [exw, 0.0])
                            {
                                cur_path = Some(*id);
                            }
                            if i != 0 && i != path.len() - 1
                            {
                                ui.same_line();
                                if ui.button(format!("x##excl{}", i))
                                {
                                    del_path = Some(*id);
                                }
                            }
                        }
                    }
                    if let Some(id) = cur_path
                    {
                        state.set_infos_current(Some(id));
                    }
                    if let Some(i) = del_path
                    {
                        state.path_dirty = true;
                        state.exclude_ids.push(i);
                    }
                }

                if ui.collapsing_header("Informations", imgui::TreeNodeFlags::empty())
                {
                    combo_with_filter(ui, "#infos_user", &mut state.infos_current, &data);
                    if let Some(id) = state.infos_current
                    {
                        let person = &data.persons[id];
                        ui.same_line();
                        if ui.button("Ouvrir")
                        {
                            // TODO: crashes on Windows because of a Winit bug
                            /*if let Err(err) = webbrowser::open(format!("https://facebook.com/{}", person.id).as_str()) {
                                log!("Couldn't open URL: {}", err);
                            };*/
                        }

                        if let Some(_t) = ui.begin_table("#infos", 2)
                        {
                            ui.table_next_row();
                            ui.table_next_column();
                            ui.text("ID Facebook :");
                            ui.table_next_column();
                            ui.text(person.id);
                            ui.table_next_column();
                            ui.text("Amis :");
                            ui.table_next_column();
                            ui.text(format!("{}", person.neighbors.len()));
                            ui.table_next_column();
                            ui.text("Classe :");
                            ui.table_next_column();
                            ui.text(format!("{}", person.modularity_class));
                        }
                    }
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

    let mut g = ImFontGlyphRangesBuilder::default();
    let mut ranges = ImVector_ImWchar::default();
    unsafe {
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

    let program = glium::Program::from_source(&display, include_str!("graph.vert"), include_str!("graph.frag"), None).unwrap();

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
