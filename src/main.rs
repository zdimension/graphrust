mod utils;
mod graph_storage;
mod camera;

use std::ffi::CString;
use std::ops::Add;
use camera::Camera;
use graph_storage::*;
use std::time::Instant;
use chrono;
use imgui::sys::{ImGuiDir, ImGuiHoveredFlags_None, ImU32, ImVec2};
use imgui::Ui;
use nalgebra::Vector2;

extern crate speedy;

use speedy::{Readable};

#[macro_use]
extern crate glium;
extern crate imgui;
extern crate imgui_glium_renderer;

use winit::dpi::PhysicalPosition;

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
}

struct UiState
{
    g_show_nodes: bool,
    g_show_edges: bool,
    infos_current: Option<usize>,
}

fn add(a: ImVec2, b: ImVec2) -> ImVec2
{
    ImVec2 { x: a.x + b.x, y: a.y + b.y }
}

unsafe fn render_arrow(draw_list: *mut imgui::sys::ImDrawList, pos: ImVec2, col: ImU32, scale: f32)
{
    let h = FONT_SIZE * 1.0;
    let r = h * 0.40 * scale;
    let center = add(pos, ImVec2 { x: h * 0.50, y: h * 0.50 * scale });

    let a = ImVec2 { x: 0.000 * r, y: 0.750 * r };
    let b = ImVec2 { x: -0.866 * r, y: -0.750 * r };
    let c = ImVec2 { x: 0.866 * r, y: -0.750 * r };

    imgui::sys::ImDrawList_AddTriangleFilled(draw_list, add(center, a), add(center, b), add(center, c), col);
}

fn fuzzy_match(pattern: &str, input: &str) -> Option<u32>
{
    Some(0)
}

unsafe fn combo_with_filter<'a>(ui: &Ui, label: &str, current_item: &mut Option<usize>, persons: &Vec<Person<'a>>) -> bool
{
    let window = imgui::sys::igGetCurrentContext();
    let storage = imgui::sys::igGetStateStorage();
    let id = imgui::sys::igGetID_Str(label.as_ptr() as _);
    let mut item_score_vector = imgui::sys::ImGuiStorage_GetVoidPtr(storage, id) as *mut Vec<(usize, u32)>;
    if item_score_vector.is_null()
    {
        let vec = Vec::new();
        item_score_vector = Box::into_raw(Box::new(vec)) as _;
        imgui::sys::ImGuiStorage_SetVoidPtr(storage, id, item_score_vector as _);
    }

    let items_count = persons.len();

    let preview_value = match current_item
    {
        Some(value) => persons[*value].name,
        None => "",
    };

    let mut pattern_buffer = String::new();
    let mut is_need_filter = false;

    let combo_button_name = format!("{}##name_ComboWithFilter_button_{}", preview_value, label);

    let name_popup = format!("##name_popup_{}", label);

    let mut value_changed = false;

    let expected_w = imgui::sys::igCalcItemWidth();
    let mut item_min = imgui::sys::ImVec2 { x: 0.0, y: 0.0 };
    imgui::sys::igGetItemRectMin(&mut item_min);
    let mut is_new_open = false;
    let sz = imgui::sys::igGetFrameHeight();
    let size = imgui::sys::ImVec2 { x: sz, y: sz };
    let mut cursor_pos = imgui::sys::ImVec2 { x: 0.0, y: 0.0 };
    imgui::sys::igGetCursorPos(&mut cursor_pos);
    let pos = add(cursor_pos, imgui::sys::ImVec2 { x: expected_w - sz, y: 0.0 });

    struct ImRect
    {
        min: imgui::sys::ImVec2,
        max: imgui::sys::ImVec2,
    }

    let bb = ImRect { min: pos, max: add(pos, size) };

    let mut style = imgui::sys::igGetStyle();
    let button_text_align_x = (*style).ButtonTextAlign.x;
    (*style).ButtonTextAlign.x = 0.0;
    if ui.button_with_size(combo_button_name, [expected_w, 0.0])
    {
        ui.open_popup(&name_popup);
        is_new_open = true;
    }
    (*style).ButtonTextAlign.x = button_text_align_x;
    let hovered = imgui::sys::igIsItemHovered(0);
    let active = imgui::sys::igIsItemActivated();
    let pressed = imgui::sys::igIsItemClicked(0);

    let text_col = imgui::sys::igGetColorU32_Col(imgui::sys::ImGuiCol_Text as i32, 1.0);
    render_arrow(
        imgui::sys::igGetWindowDrawList(),
        add(bb.min, ImVec2 { x: 0f32.max((size.x - FONT_SIZE) * 0.5), y: 0f32.max((size.y - 14.0) * 0.5) }),
        text_col,
        0.0);
    let mut item_max = imgui::sys::ImVec2 { x: 0.0, y: 0.0 };
    imgui::sys::igGetItemRectMax(&mut item_max);
    let mut content_min = imgui::sys::ImVec2 { x: 0.0, y: 0.0 };
    imgui::sys::igGetItemRectMin(&mut content_min);
    imgui::sys::igSetNextWindowPos(
        ImVec2 { x: content_min.x, y: item_max.y },
        imgui::sys::ImGuiCond_None as i32,
        ImVec2 { x: 0.0, y: 0.0 });
    let mut item_rect_size = imgui::sys::ImVec2 { x: 0.0, y: 0.0 };
    imgui::sys::igGetItemRectSize(&mut item_rect_size);
    imgui::sys::igSetNextWindowSize(
        ImVec2 { x: item_rect_size.x, y: 0.0 },
        imgui::sys::ImGuiCond_None as i32);

    ui.popup(name_popup, ||
        {
            imgui::sys::igPushStyleColor_Vec4(imgui::sys::ImGuiCol_FrameBg as i32, imgui::sys::ImVec4 { x: 240.0 / 255.0, y: 240.0 / 255.0, z: 240.0 / 255.0, w: 255.0 });
            imgui::sys::igPushStyleColor_Vec4(imgui::sys::ImGuiCol_Text as i32, imgui::sys::ImVec4 { x: 0.0, y: 0.0, z: 0.0, w: 255.0 });
            imgui::sys::igPushItemWidth(-f32::MIN_POSITIVE);

            if is_new_open
            {
                imgui::sys::igSetKeyboardFocusHere(0);
            }

            let changed = ui.input_text("##ComboWithFilter_inputText", &mut pattern_buffer).build();

            imgui::sys::igPopStyleColor(2);
            if !pattern_buffer.is_empty()
            {
                is_need_filter = true;
            }

            if changed && is_need_filter
            {
                *item_score_vector = persons.iter().enumerate()
                    .filter_map(|(i, person)|
                        {
                            if let Some(score) = fuzzy_match(pattern_buffer.as_str(), &person.name)
                            {
                                Some((i, score))
                            } else {
                                None
                            }
                        })
                    .collect();
                (*item_score_vector).sort_by(|(_, a), (_, b)| b.partial_cmp(a).unwrap());
            }

            let show_count = 100.min(if is_need_filter { (*item_score_vector).len() } else { persons.len() });
            let name = CString::new("##ComboWithFilter_itemList").unwrap();
            let height_in_items_f = show_count.min(7) as f32 + 0.25;
            if imgui::sys::igBeginListBox(
                name.as_ptr(),
                ImVec2 { x: 0.0, y: imgui::sys::igGetTextLineHeightWithSpacing() * height_in_items_f + (*style).FramePadding.y * 2.0 })
            {
                for i in 0..show_count
                {
                    let idx = if is_need_filter {
                        (*item_score_vector)[i].0
                    } else {
                        i
                    };
                    imgui::sys::igPushID_Int(idx as i32);
                    let item_selected = Some(idx) == *current_item;
                    let item_text = CString::new(persons[idx].name).expect("What");
                    if imgui::sys::igSelectable_Bool(item_text.as_ptr(), item_selected, 0, ImVec2 { x: 0.0, y: 0.0 })
                    {
                        value_changed = true;
                        *current_item = Some(idx);
                        ui.close_current_popup();
                    }
                    if item_selected
                    {
                        imgui::sys::igSetItemDefaultFocus();
                    }
                    imgui::sys::igPopID();
                }
                imgui::sys::igEndListBox();
            }
            imgui::sys::igPopItemWidth();
        });

    if value_changed
    {
        //imgui::sys::igMarkItemEdited(name_popup);
    }

    value_changed
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
                    unsafe { combo_with_filter(ui, "#infos_user", &mut state.infos_current, &data.persons); }
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

    imgui.fonts().add_font(&[
        FontSource::TtfData {
            data: include_bytes!("../Roboto-Medium.ttf"),
            size_pixels: FONT_SIZE,
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
        infos_current: None,
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
