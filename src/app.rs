use std::sync::{Arc, Mutex};
use eframe::{egui_glow, glow};
use itertools::Itertools;
use nalgebra::Matrix4;
use simsearch::SimSearch;
use crate::camera::Camera;
use crate::graph_storage::{Color3f, load_binary, Point};
use crate::log;
use crate::ui::UiState;


pub struct Person<'a>
{
    pub position: Point,
    pub size: f32,
    pub modularity_class: u16,
    pub id: &'a str,
    pub name: &'a str,
    pub sorted_id: u64,
    pub neighbors: Vec<(usize, usize)>,
}

impl<'a> Person<'a>
{
    pub fn new(position: Point, size: f32, modularity_class: u16, id: &'a str, name: &'a str) -> Person<'a>
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


#[repr(C)]
#[derive(Copy, Clone)]
pub struct Vertex
{
    pub position: Point,
    pub color: Color3f,
}

//implement_vertex!(Vertex, position, color);

impl Vertex
{
    pub fn new(position: Point, color: Color3f) -> Vertex
    {
        Vertex { position, color }
    }
}

pub struct ModularityClass<'a>
{
    pub color: Color3f,
    pub id: u16,
    pub name: String,
    pub people: Option<Vec<&'a Person<'a>>>,
}

impl<'a> ModularityClass<'a>
{
    pub fn new(color: Color3f, id: u16) -> ModularityClass<'a>
    {
        ModularityClass {
            color,
            id,
            name: format!("Classe {}", id),
            people: None,
        }
    }

    pub fn get_people(&mut self, data: &'a ViewerData<'a>) -> &Vec<&'a Person<'a>>
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

pub struct GraphViewApp<'a> {
    ui_state: UiState,
    viewer_data: ViewerData<'a>,
    rendered_graph: Arc<Mutex<RenderedGraph>>,
    camera: Camera
}

impl<'a> GraphViewApp<'a> {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let gl = cc
            .gl
            .as_ref()
            .expect("You need to run eframe with the glow backend");
        let data = load_binary();
        Self {
            ui_state: UiState::default(),
            rendered_graph: Arc::new(Mutex::new(RenderedGraph::new(gl, &data))),
            viewer_data: data,
            camera: Camera::new()
        }
    }
}

impl<'a> eframe::App for GraphViewApp<'a> {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.ui_state.draw_ui(ctx, frame, &self.viewer_data, ());

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::canvas(ui.style()).show(ui, |ui| {
                let (id, rect) = ui.allocate_space(ui.available_size());

                let sz = rect.size();
                if sz != self.camera.size {
                    self.camera.set_window_size(sz);
                }

                let response = ui.interact(rect, id, egui::Sense::click().union(egui::Sense::drag()));
                if response.dragged() {
                    self.camera.pan(response.drag_delta().x, response.drag_delta().y);
                }
                if let Some(pos) = response.hover_pos() {
                    let scroll_delta = ui.input(|is| is.scroll_delta);
                    if scroll_delta.y != 0.0 {
                        self.camera.zoom(scroll_delta.y, (pos - rect.min).to_pos2());
                    }
                }
                let cam = self.camera.get_matrix();
                let graph = self.rendered_graph.clone();
                let edges = self.ui_state.g_show_edges;
                let nodes = self.ui_state.g_show_nodes;
                let callback = egui::PaintCallback {
                    rect,
                    callback: std::sync::Arc::new(egui_glow::CallbackFn::new(move |_info, painter| {
                        graph.lock().unwrap().paint(painter.gl(), cam, edges, nodes);
                    })),
                };
                ui.painter().add(callback);
            });
        });


    }
}

struct RenderedGraph {
    program: glow::Program,
    nodes_buffer: glow::Buffer,
    nodes_count: usize,
    nodes_array: glow::VertexArray,
    edges_count: usize,
}

impl RenderedGraph {
    fn new(gl: &glow::Context, data: &ViewerData<'_>) -> Self {
        use glow::HasContext as _;

        let shader_version = if cfg!(target_arch = "wasm32") {
            "#version 300 es"
        } else {
            "#version 330"
        };

        unsafe {
            let program = gl.create_program().expect("Cannot create program");

            let shader_sources = [
                (glow::VERTEX_SHADER, include_str!("shaders/graph.vert")),
                (glow::FRAGMENT_SHADER, include_str!("shaders/graph.frag")),
            ];

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

            let mut vertices = data.persons.iter().map(|p| Vertex::new(p.position, data.modularity_classes[p.modularity_class as usize].color)).collect_vec();
            vertices.extend(&data.vertices);
            let vertices_array = gl.create_vertex_array().expect("Cannot create vertex array");
            gl.bind_vertex_array(Some(vertices_array));
            let vertices_buffer = gl.create_buffer().expect("Cannot create buffer");
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertices_buffer));
            log!("Buffering {} vertices", vertices.len());
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                std::slice::from_raw_parts(vertices.as_ptr() as *const u8, vertices.len() * std::mem::size_of::<Vertex>()),
                glow::STATIC_DRAW,
            );

            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, std::mem::size_of::<Vertex>() as i32, 0);
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, std::mem::size_of::<Vertex>() as i32, std::mem::size_of::<Point>() as i32);
            gl.enable_vertex_attrib_array(1);

            Self {
                program,
                nodes_buffer: vertices_buffer,
                nodes_count: data.persons.len(),
                nodes_array: vertices_array,
                edges_count: data.edge_sizes.len(),
            }
        }
    }

    fn destroy(&self, gl: &glow::Context) {
        use glow::HasContext as _;
        unsafe {
            gl.delete_program(self.program);
            gl.delete_buffer(self.nodes_buffer);
            gl.delete_vertex_array(self.nodes_array);
        }
    }

    fn paint(&self, gl: &glow::Context, cam: Matrix4<f32>, edges: bool, nodes: bool) {
        use glow::HasContext as _;
        unsafe {
            gl.use_program(Some(self.program));
            gl.uniform_matrix_4_f32_slice(Some(&gl.get_uniform_location(self.program, "u_projection").unwrap()), false, &cam.as_slice());
            gl.bind_vertex_array(Some(self.nodes_array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.nodes_buffer));
            if nodes {
                gl.draw_arrays(glow::POINTS, 0, self.nodes_count as i32);
            }
            if edges {
                gl.draw_arrays(glow::TRIANGLES, self.nodes_count as i32, 2 * self.edges_count as i32);
            }
        }
    }
}