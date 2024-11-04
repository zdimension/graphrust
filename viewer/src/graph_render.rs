use crate::app::ViewerData;
use crate::threading::{Cancelable, StatusWriter};
use crate::{for_progress, log};
use derivative::Derivative;
use eframe::glow;
use graph_format::nalgebra::Matrix4;
use graph_format::{Color3b, Color3f, EdgeStore, Point};
use std::collections::VecDeque;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};

pub enum GlWorkResult {
    Shaders([glow::Program; 3]),
    PathArray(glow::VertexArray, glow::Buffer),
    VertArray(glow::VertexArray, glow::Buffer),
}

pub trait GlWorkGetter<R>: FnOnce(&glow::Context) -> R {
    fn get(rx: &Receiver<GlWorkResult>) -> R;
    fn to_boxed(self) -> GlWork;
}

impl<T: Send + FnOnce(&glow::Context) -> GlWorkResult + 'static> GlWorkGetter<GlWorkResult> for T {
    fn get(rx: &Receiver<GlWorkResult>) -> GlWorkResult {
        rx.recv().unwrap()
    }

    fn to_boxed(self) -> GlWork {
        GlWork(Box::new(move |gl, tx| {
            tx.send(self(gl)).unwrap();
        }))
    }
}

impl<T: Send + FnOnce(&glow::Context) + 'static> GlWorkGetter<()> for T {
    fn get(_: &Receiver<GlWorkResult>) {}
    fn to_boxed(self) -> GlWork {
        GlWork(Box::new(move |gl, _| {
            self(gl);
        }))
    }
}

pub struct GlWork(pub(crate) Box<dyn Send + FnOnce(&glow::Context, &Sender<GlWorkResult>)>);

pub type GlMpsc = (Receiver<GlWork>, Sender<GlWorkResult>);

pub struct GlForwarder {
    tx: Sender<GlWork>,
    rx: Receiver<GlWorkResult>,
}

impl GlForwarder {
    pub fn new() -> (GlForwarder, GlMpsc) {
        let (work_tx, work_rx) = mpsc::channel();
        let (res_tx, res_rx) = mpsc::channel();
        (
            Self {
                tx: work_tx,
                rx: res_rx,
            },
            (work_rx, res_tx),
        )
    }

    pub fn run<R, T: GlWorkGetter<R>>(&self, work: T) -> R {
        self.tx.send(work.to_boxed()).unwrap();
        T::get(&self.rx)
    }
}

pub type GlTask = Box<dyn FnOnce(&mut RenderedGraph, &glow::Context) + Send + Sync + 'static>;

#[derive(Copy, Clone, Derivative)]
#[derivative(Default())]
pub struct NodeFilter {
    #[derivative(Default(value = "(0, u16::MAX)"))]
    pub degree_filter: (u16, u16),
    pub filter_nodes: bool,
}

pub struct RenderedGraph {
    pub program_node: glow::Program,
    pub program_basic: glow::Program,
    pub program_edge: glow::Program,
    pub nodes_buffer: glow::Buffer,
    pub nodes_count: usize,
    pub nodes_array: glow::VertexArray,
    pub edges_count: usize,
    pub node_filter: NodeFilter,
    pub destroyed: bool,
    pub tasks: VecDeque<GlTask>,
}

impl RenderedGraph {
    pub fn new<'a>(
        gl: GlForwarder,
        viewer: &ViewerData,
        edges: impl ExactSizeIterator<Item=&'a EdgeStore>,
        status_tx: StatusWriter,
    ) -> Cancelable<Self> {
        use glow::HasContext as _;
        use GlWorkResult::*;
        use graph_format::Point;
        use std::collections::VecDeque;
        use itertools::Itertools;
        use eframe::glow::HasContext;
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

            log!(status_tx, "Compiling shaders");
            let Shaders([program_basic, program_edge, program_node]) = gl.run(move |gl| {
                Shaders(programs.map(|shader_sources| {
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
                }))
            }) else {
                panic!("Failed to compile shaders");
            };

            #[cfg(target_arch = "wasm32")]
            let edges = edges.take(10_000_000);

            let edges_count = edges.len();
            log!(status_tx, "Creating vertice list");
            const VERTS_PER_NODE: usize = 1;
            let node_vertices = viewer
                .persons
                .iter()
                .map(|p| {
                    crate::geom_draw::create_node_vertex(p)
                });

            let edge_vertices = edges
                .map(|e| {
                    let pa = &viewer.persons[e.a as usize];
                    let pb = &viewer.persons[e.b as usize];
                    (pa, pb)
                })
                .flat_map(|(pa, pb)| {
                    crate::geom_draw::create_edge_vertices(pa, pb)
                });

            let vertices = node_vertices
                .chain(edge_vertices);

            let vertices = {
                const THRESHOLD: usize = 1024 * 1024 * 1024;
                const MAX_VERTS_IN_ONE_GIG: usize = THRESHOLD / std::mem::size_of::<PersonVertex>();
                let num_vertices = viewer.persons.len() * VERTS_PER_NODE + edges_count * crate::geom_draw::VERTS_PER_EDGE;
                if num_vertices > MAX_VERTS_IN_ONE_GIG {
                    log!(status_tx, "More than {}MB of vertices ({}), truncating", THRESHOLD / 1024 / 1024, num_vertices);
                    vertices.take(MAX_VERTS_IN_ONE_GIG).collect_vec()
                } else {
                    log!(status_tx, "Less than {}MB of vertices ({}), not truncating", THRESHOLD / 1024 / 1024, num_vertices);
                    vertices.collect_vec()
                }
            };

            let vertices_count = vertices.len();

            log!(status_tx, "Allocating vertex buffer");
            let VertArray(vertices_array, vertices_buffer) = gl.run(move |gl: &glow::Context| {
                let vertices_array = gl
                    .create_vertex_array()
                    .expect("Cannot create vertex array");
                gl.bind_vertex_array(Some(vertices_array));
                let vertices_buffer = gl.create_buffer().expect("Cannot create buffer");
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertices_buffer));
                gl.buffer_data_size(
                    glow::ARRAY_BUFFER,
                    (vertices_count * std::mem::size_of::<PersonVertex>()).try_into().unwrap(),
                    glow::STATIC_DRAW,
                );
                let err = gl.get_error();
                if err != glow::NO_ERROR {
                    log::error!("Error: {:x}", err);
                }
                gl.vertex_attrib_pointer_f32(
                    0,
                    2,
                    glow::FLOAT,
                    false,
                    size_of::<PersonVertex>() as i32,
                    0,
                );
                gl.enable_vertex_attrib_array(0);
                gl.vertex_attrib_pointer_i32(
                    1,
                    1,
                    glow::UNSIGNED_INT,
                    size_of::<PersonVertex>() as i32,
                    size_of::<Point>() as i32,
                );
                gl.enable_vertex_attrib_array(1);

                VertArray(vertices_array, vertices_buffer)
            }) else {
                panic!("Failed to create vertices array");
            };

            log!(status_tx, "Buffering {} vertices", vertices.len());

            let vertices = std::sync::Arc::new(vertices);

            const BATCH_SIZE: usize = 1000000;

            for_progress!(status_tx, i in 0..vertices.len().div_ceil(BATCH_SIZE), {
                let vertices = vertices.clone();
                gl.run(move |gl: &glow::Context| {
                    let start = i * BATCH_SIZE;
                    let end = ((i + 1) * BATCH_SIZE).min(vertices.len());
                    let batch = &vertices[i * BATCH_SIZE..end];
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertices_buffer));
                    gl.buffer_sub_data_u8_slice(
                        glow::ARRAY_BUFFER,
                        (start * std::mem::size_of::<PersonVertex>()).try_into().unwrap(),
                        std::slice::from_raw_parts(
                            batch.as_ptr() as *const u8,
                            std::mem::size_of_val(batch),
                        ),
                    );
                    let err = gl.get_error();
                    if err != glow::NO_ERROR {
                        log::error!("Error: {:x}", err);
                    }
                });
            });

            log!(status_tx, "Creating path array");
            let PathArray(path_array, path_buffer) = gl.run(|gl| {
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
                    size_of::<Vertex>() as i32,
                    0,
                );
                gl.enable_vertex_attrib_array(0);
                gl.vertex_attrib_pointer_f32(
                    1,
                    3,
                    glow::UNSIGNED_BYTE,
                    true,
                    size_of::<Vertex>() as i32,
                    size_of::<Point>() as i32,
                );
                gl.enable_vertex_attrib_array(1);

                PathArray(path_array, path_buffer)
            }) else {
                panic!("Failed to create path array");
            };

            log!(status_tx, "Done: {}", chrono::Local::now().format("%H:%M:%S.%3f"));

            Ok(Self {
                program_basic,
                program_edge,
                program_node,
                nodes_buffer: vertices_buffer,
                nodes_count: viewer.persons.len(),
                nodes_array: vertices_array,
                edges_count,
                node_filter: NodeFilter::default(),
                destroyed: false,
                tasks: VecDeque::new(),
            })
        }
    }

    pub(crate) fn destroy(&mut self, gl: &glow::Context) {
        log::info!("Destroying graph");
        self.destroyed = true;
        use eframe::glow::HasContext;
        use glow::HasContext as _;
        unsafe {
            log::info!("Deleting programs");
            gl.delete_program(self.program_basic);
            gl.delete_program(self.program_edge);
            gl.delete_program(self.program_node);
            log::info!("Deleting buffers");
            gl.delete_buffer(self.nodes_buffer);
            log::info!("Deleting arrays");
            gl.delete_vertex_array(self.nodes_array);
        }
    }

    pub(crate) fn paint(
        &mut self,
        gl: &glow::Context,
        cam: Matrix4<f32>,
        edges: (bool, f32),
        nodes: (bool, f32),
        class_colors: &[Color3f],
    ) {
        if self.destroyed {
            return;
        }

        while let Some(task) = self.tasks.pop_front() {
            task(self, gl);
        }

        use eframe::glow::HasContext;
        use glow::HasContext as _;
        unsafe {
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

            gl.bind_vertex_array(Some(self.nodes_array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.nodes_buffer));

            let mut all_colors = [Color3f::new(0.0, 0.0, 0.0); 512];
            all_colors[..class_colors.len()].copy_from_slice(class_colors);

            if edges.0 {
                gl.use_program(Some(self.program_edge));
                gl.uniform_matrix_4_f32_slice(
                    Some(
                        &gl.get_uniform_location(self.program_edge, "u_projection")
                            .unwrap(),
                    ),
                    false,
                    cam.as_slice(),
                );
                gl.uniform_1_u32(
                    Some(
                        &gl.get_uniform_location(self.program_edge, "u_degfilter")
                            .unwrap(),
                    ),
                    ((self.node_filter.degree_filter.1 as u32) << 16) | (self.node_filter.degree_filter.0 as u32),
                );
                gl.uniform_1_f32(
                    Some(
                        &gl.get_uniform_location(self.program_edge, "opacity")
                            .unwrap(),
                    ),
                    edges.1,
                );

                gl.uniform_3_f32_slice(
                    Some(
                        &gl.get_uniform_location(self.program_edge, "u_class_colors")
                            .unwrap(),
                    ),
                    // SAFETY: Color3f is repr(C) and is equivalent to [f32; 3]
                    unsafe { std::slice::from_raw_parts(all_colors.as_ptr() as *const f32, 512 * 3) },
                );
                let verts = 2 * 3 * self.edges_count as i32;
                // if wasm, clamp verts at 30M, because Firefox refuses to draw anything above that
                #[cfg(target_arch = "wasm32")]
                let verts = verts.min(30_000_000);
                gl.draw_arrays(glow::TRIANGLES, self.nodes_count as i32, verts);
            }
            if nodes.0 {
                gl.use_program(Some(self.program_node));
                gl.uniform_matrix_4_f32_slice(
                    Some(
                        &gl.get_uniform_location(self.program_node, "u_projection")
                            .unwrap(),
                    ),
                    false,
                    cam.as_slice(),
                );
                gl.uniform_1_u32(
                    Some(
                        &gl.get_uniform_location(self.program_node, "u_degfilter")
                            .unwrap(),
                    ),
                    if self.node_filter.filter_nodes {
                        ((self.node_filter.degree_filter.1 as u32) << 16) | (self.node_filter.degree_filter.0 as u32)
                    } else {
                        0xffff_0000
                    },
                );
                gl.uniform_1_f32(
                    Some(
                        &gl.get_uniform_location(self.program_node, "opacity")
                            .unwrap(),
                    ),
                    nodes.1,
                );

                gl.uniform_3_f32_slice(
                    Some(
                        &gl.get_uniform_location(self.program_node, "u_class_colors")
                            .unwrap(),
                    ),
                    unsafe { std::slice::from_raw_parts(all_colors.as_ptr() as *const f32, 512 * 3) },
                );
                gl.draw_arrays(glow::POINTS, 0, self.nodes_count as i32);
            }
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Vertex {
    pub position: Point,
    pub color: Color3b,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct PersonVertex {
    pub position: Point,
    pub degree_and_class: u32,
}

impl PersonVertex {
    pub fn new(position: Point, degree: u16, class: u16) -> PersonVertex {
        PersonVertex {
            position,
            degree_and_class: ((class as u32) << 16) | (degree as u32),
        }
    }
}

impl Vertex {
    pub fn new(position: Point, color: Color3b) -> Vertex {
        Vertex { position, color }
    }
}