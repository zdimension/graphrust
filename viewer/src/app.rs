use crate::camera::Camera;

use crate::graph_storage::{load_binary, ProcessedData};
use crate::ui::UiState;
use eframe::{egui_glow, glow};
use egui::{Id, Vec2};
use graph_format::{Color3f, Point};
use itertools::Itertools;
use nalgebra::{Matrix4, Vector4};
use simsearch::SimSearch;
use std::ops::Range;
use std::sync::{Arc, Mutex};

pub struct Person<'a> {
    pub position: Point,
    pub size: f32,
    pub modularity_class: u16,
    pub id: &'a str,
    pub name: &'a str,
    pub sorted_id: u64,
    pub neighbors: Vec<(usize, usize)>,
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

pub struct ModularityClass<'a> {
    pub color: Color3f,
    pub id: u16,
    pub name: String,
    pub people: Option<Vec<&'a Person<'a>>>,
}

impl<'a> ModularityClass<'a> {
    pub fn new(color: Color3f, id: u16) -> ModularityClass<'a> {
        ModularityClass {
            color,
            id,
            name: format!("Classe {}", id),
            people: None,
        }
    }

    pub fn get_people(&mut self, data: &'a ViewerData<'a>) -> &Vec<&'a Person<'a>> {
        match self.people {
            Some(ref people) => people,
            None => {
                let filtered = data
                    .persons
                    .iter()
                    .filter(|p| p.modularity_class == self.id)
                    .collect();
                self.people = Some(filtered);
                self.people.as_ref().unwrap()
            }
        }
    }
}

pub struct ViewerData<'a> {
    pub ids: Vec<u8>,
    pub names: Vec<u8>,
    pub persons: Vec<Person<'a>>,
    pub modularity_classes: Vec<ModularityClass<'a>>,
    pub engine: SimSearch<usize>,
}

pub struct GraphViewApp<'a> {
    ui_state: UiState,
    viewer_data: ViewerData<'a>,
    rendered_graph: Arc<Mutex<RenderedGraph>>,
    camera: Camera,
    cam_animating: Option<Vec2>,
}

impl<'a> GraphViewApp<'a> {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let gl = cc
            .gl
            .as_ref()
            .expect("You need to run eframe with the glow backend");
        let data = load_binary();
        let min = data
            .viewer
            .persons
            .iter()
            .map(|p| p.position)
            .reduce(|a, b| Point {
                x: a.x.min(b.x),
                y: a.y.min(b.y),
            })
            .unwrap();
        let max = data
            .viewer
            .persons
            .iter()
            .map(|p| p.position)
            .reduce(|a, b| Point {
                x: a.x.max(b.x),
                y: a.y.max(b.y),
            })
            .unwrap();
        log::info!("min: {:?}, max: {:?}", min, max);
        let center = min + (max - min) / 2.0;
        let mut res = Self {
            ui_state: UiState {
                node_count: data.viewer.persons.len(),
                ..UiState::default()
            },
            rendered_graph: Arc::new(Mutex::new(RenderedGraph::new(gl, &data))),
            viewer_data: data.viewer,
            camera: Camera::new(),
            cam_animating: None,
        };
        //res.camera.pan(-center.x, -center.y);
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
        res
    }
}

impl<'a> eframe::App for GraphViewApp<'a> {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let cid = Id::from("camera");

        self.ui_state.draw_ui(ctx, frame, &self.viewer_data);
        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                let (id, rect) = ui.allocate_space(ui.available_size());

                let sz = rect.size();
                if sz != self.camera.size {
                    self.camera.set_window_size(sz);
                }

                let response =
                    ui.interact(rect, id, egui::Sense::click().union(egui::Sense::drag()));

                if !response.is_pointer_button_down_on() {
                    if let Some(v) = self.cam_animating {
                        let anim = ctx.animate_bool_with_time(cid, false, 0.5);
                        if anim == 0.0 {
                            self.cam_animating = None;
                        } else {
                            let v = v * anim;
                            self.camera.pan(v.x, v.y);
                        }
                    }
                }

                if response.dragged() {
                    self.camera
                        .pan(response.drag_delta().x, response.drag_delta().y);

                    ctx.animate_bool_with_time(cid, true, 0.0);
                    self.cam_animating = Some(response.drag_delta());
                }

                if let Some(pos) = response.hover_pos() {
                    let zero_pos = (pos - rect.min).to_pos2();
                    let centered_pos = (pos - rect.center()) / rect.size();
                    self.ui_state.mouse_pos = Some(centered_pos.to_pos2());
                    self.ui_state.mouse_pos_world = Some(
                        (self.camera.get_inverse_matrix()
                            * Vector4::new(centered_pos.x, -centered_pos.y, 0.0, 1.0))
                        .xy(),
                    );
                    let scroll_delta = ui.input(|is| is.scroll_delta);
                    if scroll_delta.y != 0.0 {
                        self.camera.zoom(scroll_delta.y, zero_pos);
                    }
                } else {
                    self.ui_state.mouse_pos = None;
                    self.ui_state.mouse_pos_world = None;
                }

                let graph = self.rendered_graph.clone();
                let edges = self.ui_state.g_show_edges;
                let nodes = self.ui_state.g_show_nodes;

                if let Some(path) = self.ui_state.path_vbuf.take() {
                    graph.lock().unwrap().new_path = Some(path);
                }

                if self.ui_state.deg_filter_changed {
                    graph.lock().unwrap().degree_filter = self.ui_state.deg_filter;
                    self.ui_state.deg_filter_changed = false;
                }

                let cam = self.camera.get_matrix();
                let callback = egui::PaintCallback {
                    rect,
                    callback: std::sync::Arc::new(egui_glow::CallbackFn::new(
                        move |_info, painter| {
                            graph.lock().unwrap().paint(painter.gl(), cam, edges, nodes);
                        },
                    )),
                };
                ui.painter().add(callback);
            });
    }
}

pub struct RenderedGraph {
    program_node: glow::Program,
    program_person: glow::Program,
    nodes_buffer: glow::Buffer,
    nodes_count: usize,
    nodes_array: glow::VertexArray,
    edges_count: usize,
    path_array: glow::VertexArray,
    path_buffer: glow::Buffer,
    path_count: usize,
    new_path: Option<Vec<Vertex>>,
    degree_filter: (u16, u16),
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
                    (glow::VERTEX_SHADER, include_str!("shaders/graph.vert")),
                    (glow::FRAGMENT_SHADER, include_str!("shaders/graph.frag")),
                ],
                [
                    (glow::VERTEX_SHADER, include_str!("shaders/person.vert")),
                    (glow::FRAGMENT_SHADER, include_str!("shaders/graph.frag")),
                ],
            ];

            let [program_node, program_person] = programs.map(|shader_sources| {
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
                .chain(data.edges.iter().flat_map(|e| {
                    let pa = &data.viewer.persons[e.a as usize];
                    let pb = &data.viewer.persons[e.b as usize];
                    let a = pa.position;
                    let b = pb.position;
                    let ortho = (b - a).ortho().normalized() * 1.0;
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
                }))
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
                program_node,
                program_person,
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
            gl.delete_program(self.program_node);
            gl.delete_program(self.program_person);
            gl.delete_buffer(self.nodes_buffer);
            gl.delete_vertex_array(self.nodes_array);
            gl.delete_buffer(self.path_buffer);
            gl.delete_vertex_array(self.path_array);
        }
    }

    fn paint(&mut self, gl: &glow::Context, cam: Matrix4<f32>, edges: bool, nodes: bool) {
        use glow::HasContext as _;
        unsafe {
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.use_program(Some(self.program_person));
            gl.uniform_matrix_4_f32_slice(
                Some(
                    &gl.get_uniform_location(self.program_person, "u_projection")
                        .unwrap(),
                ),
                false,
                &cam.as_slice(),
            );
            gl.uniform_1_u32(
                Some(
                    &gl.get_uniform_location(self.program_person, "u_degfilter")
                        .unwrap(),
                ),
                ((self.degree_filter.1 as u32) << 16) | (self.degree_filter.0 as u32),
            );
            gl.bind_vertex_array(Some(self.nodes_array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.nodes_buffer));
            if edges {
                gl.draw_arrays(
                    glow::TRIANGLES,
                    self.nodes_count as i32,
                    2 * 3 * self.edges_count as i32,
                );
            }
            if nodes {
                gl.draw_arrays(glow::POINTS, 0, self.nodes_count as i32);
            }

            gl.use_program(Some(self.program_node));
            gl.uniform_matrix_4_f32_slice(
                Some(
                    &gl.get_uniform_location(self.program_node, "u_projection")
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
