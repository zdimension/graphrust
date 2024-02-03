use crate::camera::Camera;
use std::marker::PhantomData;

use crate::graph_storage::{load_binary, ProcessedData};
use crate::ui::UiState;
use eframe::glow::HasContext;
use eframe::{egui_glow, glow};
use egui::{Color32, Id, RichText, Ui, Vec2, WidgetText};
use egui_dock::{DockArea, DockState, Style};
use graph_format::{Color3f, Point};
use itertools::Itertools;
use nalgebra::{Matrix4, Vector4};
use simsearch::SimSearch;
use std::ops::Range;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Person<'a> {
    pub position: Point,
    pub size: f32,
    pub modularity_class: u16,
    pub id: &'a str,
    pub name: &'a str,
    pub sorted_id: u64,
    pub neighbors: Vec<usize>,
}

impl<'a> Person<'a> {
    pub fn new(
        position: Point,
        size: f32,
        modularity_class: u16,
        id: &'a str,
        name: &'a str,
    ) -> Person<'a> {
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

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Vertex {
    pub position: Point,
    pub color: Color3f,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct PersonVertex {
    pub vertex: Vertex,
    pub degree_and_class: u32,
}

impl PersonVertex {
    pub fn new(position: Point, color: Color3f, degree: u16, class: u16) -> PersonVertex {
        PersonVertex {
            vertex: Vertex::new(position, color),
            degree_and_class: ((class as u32) << 16) | (degree as u32),
        }
    }
}
//implement_vertex!(Vertex, position, color);

impl Vertex {
    pub fn new(position: Point, color: Color3f) -> Vertex {
        Vertex { position, color }
    }
}

#[derive(Clone)]
pub struct ModularityClass {
    pub color: Color3f,
    pub id: u16,
    pub name: String,
}

impl ModularityClass {
    pub fn new(color: Color3f, id: u16) -> ModularityClass {
        ModularityClass {
            color,
            id,
            name: format!("Classe {}", id),
        }
    }

    /*pub fn get_people<'a>(&mut self, data: &ViewerData<'a>) -> &Vec<&Person<'a>> {
        data.persons
            .iter()
            .filter(|p| p.modularity_class == self.id)
            .collect()
    }*/
}

#[derive(Clone)]
pub struct ViewerData<'a> {
    pub persons: Vec<Person<'a>>,
    pub modularity_classes: Vec<ModularityClass>,
    pub engine: SimSearch<usize>,
}

pub struct StringTables {
    pub ids: Vec<u8>,
    pub names: Vec<u8>,
}

pub struct GraphTab<'a> {
    ui_state: UiState,
    viewer_data: ViewerData<'a>,
    rendered_graph: Arc<Mutex<RenderedGraph>>,
    camera: Camera,
    cam_animating: Option<Vec2>,
    closeable: bool,
    pub title: String,
}

pub struct GraphViewApp<'a> {
    tree: DockState<GraphTab<'a>>,
    string_tables: StringTables,
}

impl<'a> GraphViewApp<'a> {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let gl = cc
            .gl
            .as_ref()
            .expect("You need to run eframe with the glow backend");
        unsafe {
            gl.enable(glow::PROGRAM_POINT_SIZE);
        }
        let data = load_binary();
        log::info!("Computing center");
        let center = data
            .viewer
            .persons
            .iter()
            .map(|p| p.position)
            .sum::<Point>()
            / data.viewer.persons.len() as f32;
        log::info!("Center: {:?}", center);
        let mut default_tab = GraphTab {
            ui_state: UiState {
                node_count: data.viewer.persons.len(),
                g_opac_edges: 300000.0 / data.edges.len() as f32,
                g_opac_nodes: 40000.0 / data.viewer.persons.len() as f32,
                max_degree: data
                    .viewer
                    .persons
                    .iter()
                    .map(|p| p.neighbors.len())
                    .max()
                    .unwrap() as u16,
                ..UiState::default()
            },
            rendered_graph: Arc::new(Mutex::new(RenderedGraph::new(gl, &data))),
            viewer_data: data.viewer.clone(),
            camera: Camera::new(center.into()),
            cam_animating: None,
            closeable: false,
            title: String::from("Graphe"),
        };
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;

            // set #center_text.innerHTML to ""
            eframe::web_sys::window()
                .unwrap()
                .document()
                .unwrap()
                .get_element_by_id("center_text")
                .unwrap()
                .dyn_ref::<eframe::web_sys::HtmlElement>()
                .unwrap()
                .set_inner_html("");
        }
        let mut res = Self {
            tree: DockState::new(vec![default_tab]),
            string_tables: data.strings,
        };
        res
    }
}

struct TabViewer<'a, 'b> {
    ctx: &'b egui::Context,
    data: PhantomData<&'a bool>,
}

impl<'a, 'b> egui_dock::TabViewer for TabViewer<'a, 'b> {
    type Tab = GraphTab<'a>;

    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        RichText::from(&tab.title).into()
    }

    fn closeable(&mut self, tab: &mut Self::Tab) -> bool {
        tab.closeable
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        let ctx = self.ctx;
        let cid = Id::from("camera");
        tab.ui_state.draw_ui(
            ui,
            &tab.viewer_data,
            &mut *tab.rendered_graph.lock().unwrap(),
        );
        egui::CentralPanel::default()
            .frame(egui::Frame {
                fill: Color32::WHITE,
                ..Default::default()
            })
            .show_inside(ui, |ui| {
                let (id, rect) = ui.allocate_space(ui.available_size());

                let sz = rect.size();
                if sz != tab.camera.size {
                    tab.camera.set_window_size(sz);
                }

                let response =
                    ui.interact(rect, id, egui::Sense::click().union(egui::Sense::drag()));

                if !response.is_pointer_button_down_on() {
                    if let Some(v) = tab.cam_animating {
                        let anim = ctx.animate_bool_with_time(cid, false, 0.5);
                        if anim == 0.0 {
                            tab.cam_animating = None;
                        } else {
                            let v = v * anim;
                            tab.camera.pan(v.x, v.y);
                        }
                    }
                }

                if response.dragged() {
                    tab.camera
                        .pan(response.drag_delta().x, response.drag_delta().y);

                    ctx.animate_bool_with_time(cid, true, 0.0);
                    tab.cam_animating = Some(response.drag_delta());
                }

                if let Some(pos) = response.hover_pos() {
                    let zero_pos = (pos - rect.min).to_pos2();
                    let centered_pos = (pos - rect.center()) / rect.size();
                    tab.ui_state.mouse_pos = Some(centered_pos.to_pos2());
                    tab.ui_state.mouse_pos_world = Some(
                        (tab.camera.get_inverse_matrix()
                            * Vector4::new(centered_pos.x, -centered_pos.y, 0.0, 1.0))
                        .xy(),
                    );
                    let scroll_delta = ui.input(|is| is.scroll_delta);
                    if scroll_delta.y != 0.0 {
                        tab.camera.zoom(scroll_delta.y, zero_pos);
                    }
                } else {
                    tab.ui_state.mouse_pos = None;
                    tab.ui_state.mouse_pos_world = None;
                }

                let graph = tab.rendered_graph.clone();
                let edges = tab.ui_state.g_show_edges;
                let nodes = tab.ui_state.g_show_nodes;
                let opac_edges = tab.ui_state.g_opac_edges;
                let opac_nodes = tab.ui_state.g_opac_nodes;

                let cam = tab.camera.get_matrix();
                tab.ui_state.camera = cam;
                let callback = egui::PaintCallback {
                    rect,
                    callback: std::sync::Arc::new(egui_glow::CallbackFn::new(
                        move |_info, painter| {
                            graph.lock().unwrap().paint(
                                painter.gl(),
                                cam,
                                (edges, opac_edges),
                                (nodes, opac_nodes),
                            );
                        },
                    )),
                };
                ui.painter().add(callback);
            });
    }
}

impl<'a> eframe::App for GraphViewApp<'a> {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        DockArea::new(&mut self.tree)
            .style({
                let style = Style::from_egui(ctx.style().as_ref());
                style
            })
            .show(
                ctx,
                &mut TabViewer {
                    ctx,
                    data: PhantomData,
                },
            );
    }
}

pub struct RenderedGraph {
    pub program_node: glow::Program,
    pub program_basic: glow::Program,
    pub program_edge: glow::Program,
    pub nodes_buffer: glow::Buffer,
    pub nodes_count: usize,
    pub nodes_array: glow::VertexArray,
    pub edges_count: usize,
    pub path_array: glow::VertexArray,
    pub path_buffer: glow::Buffer,
    pub path_count: usize,
    pub new_path: Option<Vec<Vertex>>,
    pub degree_filter: (u16, u16),
}

impl RenderedGraph {
    fn new(gl: &glow::Context, data: &ProcessedData<'_>) -> Self {
        use glow::HasContext as _;

        let shader_version = if cfg!(target_arch = "wasm32") {
            "#version 300 es"
        } else {
            "#version 330"
        };

        unsafe {
            let programs = [
                [
                    (glow::VERTEX_SHADER, include_str!("shaders/basic.vert")),
                    (glow::FRAGMENT_SHADER, include_str!("shaders/basic.frag")),
                ],
                [
                    (glow::VERTEX_SHADER, include_str!("shaders/graph.vert")),
                    (
                        glow::FRAGMENT_SHADER,
                        include_str!("shaders/graph_edge.frag"),
                    ),
                ],
                [
                    (glow::VERTEX_SHADER, include_str!("shaders/graph.vert")),
                    (
                        glow::FRAGMENT_SHADER,
                        include_str!("shaders/graph_node.frag"),
                    ),
                ],
            ];

            let [program_basic, program_edge, program_node] = programs.map(|shader_sources| {
                let program = gl.create_program().expect("Cannot create program");

                let shaders: Vec<_> = shader_sources
                    .iter()
                    .map(|(shader_type, shader_source)| {
                        let shader = gl
                            .create_shader(*shader_type)
                            .expect("Cannot create shader");
                        gl.shader_source(shader, &format!("{shader_version}\n{shader_source}"));
                        gl.compile_shader(shader);
                        assert!(
                            gl.get_shader_compile_status(shader),
                            "Failed to compile {shader_type}: {}",
                            gl.get_shader_info_log(shader)
                        );
                        gl.attach_shader(program, shader);
                        shader
                    })
                    .collect();

                gl.link_program(program);
                assert!(
                    gl.get_program_link_status(program),
                    "{}",
                    gl.get_program_info_log(program)
                );

                for shader in shaders {
                    gl.detach_shader(program, shader);
                    gl.delete_shader(shader);
                }

                program
            });

            let vertices = data
                .viewer
                .persons
                .iter()
                .map(|p| {
                    PersonVertex::new(
                        p.position,
                        data.viewer.modularity_classes[p.modularity_class as usize].color,
                        p.neighbors.len() as u16,
                        p.modularity_class,
                    )
                })
                .chain(
                    data.edges
                        .iter()
                        .map(|e| {
                            let pa = &data.viewer.persons[e.a as usize];
                            let pb = &data.viewer.persons[e.b as usize];
                            (pa, pb)
                        })
                        //.filter(|(pa, pb)| pa.neighbors.len() > 5 && pb.neighbors.len() > 5)
                        .flat_map(|(pa, pb)| {
                            let a = pa.position;
                            let b = pb.position;
                            let ortho = (b - a).ortho().normalized() * 0.75;
                            let v0 = a + ortho;
                            let v1 = a - ortho;
                            let v2 = b - ortho;
                            let v3 = b + ortho;
                            let color_a =
                                data.viewer.modularity_classes[pa.modularity_class as usize].color;
                            let color_b =
                                data.viewer.modularity_classes[pb.modularity_class as usize].color;
                            [
                                PersonVertex::new(
                                    v0,
                                    color_a,
                                    pa.neighbors.len() as u16,
                                    pa.modularity_class,
                                ),
                                PersonVertex::new(
                                    v1,
                                    color_a,
                                    pa.neighbors.len() as u16,
                                    pa.modularity_class,
                                ),
                                PersonVertex::new(
                                    v2,
                                    color_b,
                                    pb.neighbors.len() as u16,
                                    pb.modularity_class,
                                ),
                                PersonVertex::new(
                                    v2,
                                    color_b,
                                    pb.neighbors.len() as u16,
                                    pb.modularity_class,
                                ),
                                PersonVertex::new(
                                    v3,
                                    color_b,
                                    pb.neighbors.len() as u16,
                                    pb.modularity_class,
                                ),
                                PersonVertex::new(
                                    v0,
                                    color_a,
                                    pa.neighbors.len() as u16,
                                    pa.modularity_class,
                                ),
                            ]
                        }),
                )
                .collect_vec();

            let vertices_array = gl
                .create_vertex_array()
                .expect("Cannot create vertex array");
            gl.bind_vertex_array(Some(vertices_array));
            let vertices_buffer = gl.create_buffer().expect("Cannot create buffer");
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertices_buffer));
            log::info!("Buffering {} vertices", vertices.len());
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                std::slice::from_raw_parts(
                    vertices.as_ptr() as *const u8,
                    vertices.len() * std::mem::size_of::<PersonVertex>(),
                ),
                glow::STATIC_DRAW,
            );

            gl.vertex_attrib_pointer_f32(
                0,
                2,
                glow::FLOAT,
                false,
                std::mem::size_of::<PersonVertex>() as i32,
                0,
            );
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(
                1,
                3,
                glow::FLOAT,
                false,
                std::mem::size_of::<PersonVertex>() as i32,
                std::mem::size_of::<Point>() as i32,
            );
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_i32(
                2,
                1,
                glow::UNSIGNED_INT,
                std::mem::size_of::<PersonVertex>() as i32,
                std::mem::size_of::<Vertex>() as i32,
            );
            gl.enable_vertex_attrib_array(2);

            let path_array = gl
                .create_vertex_array()
                .expect("Cannot create vertex array");
            let path_buffer = gl.create_buffer().expect("Cannot create buffer");

            gl.bind_vertex_array(Some(path_array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(path_buffer));

            gl.vertex_attrib_pointer_f32(
                0,
                2,
                glow::FLOAT,
                false,
                std::mem::size_of::<Vertex>() as i32,
                0,
            );
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(
                1,
                3,
                glow::FLOAT,
                false,
                std::mem::size_of::<Vertex>() as i32,
                std::mem::size_of::<Point>() as i32,
            );
            gl.enable_vertex_attrib_array(1);

            Self {
                program_basic,
                program_edge,
                program_node,
                nodes_buffer: vertices_buffer,
                nodes_count: data.viewer.persons.len(),
                nodes_array: vertices_array,
                edges_count: data.edges.len(),
                path_array,
                path_buffer,
                path_count: 0,
                new_path: None,
                degree_filter: (0, u16::MAX),
            }
        }
    }

    fn destroy(&self, gl: &glow::Context) {
        use glow::HasContext as _;
        unsafe {
            gl.delete_program(self.program_basic);
            gl.delete_program(self.program_edge);
            gl.delete_program(self.program_node);
            gl.delete_buffer(self.nodes_buffer);
            gl.delete_vertex_array(self.nodes_array);
            gl.delete_buffer(self.path_buffer);
            gl.delete_vertex_array(self.path_array);
        }
    }

    fn paint(
        &mut self,
        gl: &glow::Context,
        cam: Matrix4<f32>,
        edges: (bool, f32),
        nodes: (bool, f32),
    ) {
        use glow::HasContext as _;
        unsafe {
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

            gl.bind_vertex_array(Some(self.nodes_array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.nodes_buffer));
            if edges.0 {
                gl.use_program(Some(self.program_edge));
                gl.uniform_matrix_4_f32_slice(
                    Some(
                        &gl.get_uniform_location(self.program_edge, "u_projection")
                            .unwrap(),
                    ),
                    false,
                    &cam.as_slice(),
                );
                gl.uniform_1_u32(
                    Some(
                        &gl.get_uniform_location(self.program_edge, "u_degfilter")
                            .unwrap(),
                    ),
                    ((self.degree_filter.1 as u32) << 16) | (self.degree_filter.0 as u32),
                );
                gl.uniform_1_f32(
                    Some(
                        &gl.get_uniform_location(self.program_edge, "opacity")
                            .unwrap(),
                    ),
                    edges.1,
                );
                gl.draw_arrays(
                    glow::TRIANGLES,
                    self.nodes_count as i32,
                    2 * 3 * self.edges_count as i32,
                );
            }
            if nodes.0 {
                gl.use_program(Some(self.program_node));
                gl.uniform_matrix_4_f32_slice(
                    Some(
                        &gl.get_uniform_location(self.program_node, "u_projection")
                            .unwrap(),
                    ),
                    false,
                    &cam.as_slice(),
                );
                gl.uniform_1_u32(
                    Some(
                        &gl.get_uniform_location(self.program_node, "u_degfilter")
                            .unwrap(),
                    ),
                    ((self.degree_filter.1 as u32) << 16) | (self.degree_filter.0 as u32),
                );
                gl.uniform_1_f32(
                    Some(
                        &gl.get_uniform_location(self.program_node, "opacity")
                            .unwrap(),
                    ),
                    nodes.1,
                );
                gl.draw_arrays(glow::POINTS, 0, self.nodes_count as i32);
            }

            gl.use_program(Some(self.program_basic));
            gl.uniform_matrix_4_f32_slice(
                Some(
                    &gl.get_uniform_location(self.program_basic, "u_projection")
                        .unwrap(),
                ),
                false,
                &cam.as_slice(),
            );
            gl.bind_vertex_array(Some(self.path_array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.path_buffer));
            if let Some(path) = self.new_path.take() {
                log::info!("Buffering {} path vertices", path.len());
                if path.is_empty() {
                    self.path_count = 0;
                } else {
                    gl.buffer_data_u8_slice(
                        glow::ARRAY_BUFFER,
                        std::slice::from_raw_parts(
                            path.as_ptr() as *const u8,
                            path.len() * std::mem::size_of::<Vertex>(),
                        ),
                        glow::STATIC_DRAW,
                    );
                    self.path_count = path.len();
                }
            }

            if self.path_count > 0 {
                gl.draw_arrays(glow::TRIANGLES, 0, self.path_count as i32);
            }
        }
    }
}
