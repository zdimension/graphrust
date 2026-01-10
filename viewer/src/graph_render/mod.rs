use crate::app::ViewerData;
use crate::threading::{Cancelable, StatusWriter};
use crate::{for_progress, log};
use anyhow::anyhow;
use derivative::Derivative;
use eframe::glow;
use graph_format::nalgebra::Matrix4;
use graph_format::{Color3b, Color3f, EdgeStore, Point};
use std::collections::VecDeque;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use rayon::prelude::*;

pub mod camera;
pub mod geom_draw;

pub type GlWorkResult = Box<dyn std::any::Any + Send>;

pub struct GlWork(pub(crate) Box<dyn Send + FnOnce(&glow::Context, &Sender<GlWorkResult>)>);

pub type GlMpsc = (Receiver<GlWork>, Sender<GlWorkResult>);

/// A forwarder for sending work to the GL thread
///
/// This is a simple wrapper around an MPSC channel that allows sending work to the GL thread.
/// Internally, it sends a boxed closure that is run on the next frame, and returns the result
/// inside a Box<dyn Any>.
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

    pub fn run<R: Send + 'static, T: FnOnce(&glow::Context) -> R + Send + 'static>(
        &self,
        work: T,
    ) -> Cancelable<R> {
        self.tx.send(GlWork(Box::new(move |gl, tx| {
            tx.send(Box::new(work(gl))).unwrap();
        })))?;
        Ok(*self
            .rx
            .recv()?
            .downcast()
            .map_err(|_| anyhow!("Failed to downcast"))?)
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
    pub nodes_instance_buffer: glow::Buffer,
    pub nodes_count: usize,
    pub quad_vao: glow::VertexArray,
    pub quad_vbo: glow::Buffer,
    pub edge_quad_vao: glow::VertexArray,
    pub edge_quad_vbo: glow::Buffer,
    pub edge_instance_buffer: glow::Buffer,
    pub edges_count: usize,
    pub node_filter: NodeFilter,
    pub destroyed: bool,
    pub tasks: VecDeque<GlTask>,
}

impl RenderedGraph {
    pub const MAX_RENDERED_EDGES: usize = if cfg!(target_arch = "wasm32") {
        1_000_000
    } else {
        10_000_000
    };
    
    pub fn new<'a>(
        gl: GlForwarder,
        viewer: &ViewerData,
        edges: Vec<EdgeStore>,
        status_tx: StatusWriter,
    ) -> Cancelable<Self> {
        use eframe::glow::HasContext;
        use glow::HasContext as _;
        use graph_format::Point;
        use itertools::Itertools;
        use std::collections::VecDeque;
        let shader_version = if cfg!(target_arch = "wasm32") {
            "#version 300 es"
        } else {
            "#version 330"
        };

        unsafe {
            let common_glsl = include_str!("shaders/common.glsl");
            
            let programs = [
                [
                    (glow::VERTEX_SHADER, include_str!("shaders/basic.vert")),
                    (glow::FRAGMENT_SHADER, include_str!("shaders/basic.frag")),
                ],
                [
                    (glow::VERTEX_SHADER, include_str!("shaders/graph_edge.vert")),
                    (
                        glow::FRAGMENT_SHADER,
                        include_str!("shaders/graph_edge.frag"),
                    ),
                ],
                [
                    (glow::VERTEX_SHADER, include_str!("shaders/graph_node.vert")),
                    (
                        glow::FRAGMENT_SHADER,
                        include_str!("shaders/graph_node.frag"),
                    ),
                ],
            ];

            log!(status_tx, t!("Compiling shaders"));
            let num_classes = viewer.modularity_classes.len();
            let [program_basic, program_edge, program_node] = gl.run(move |gl| {
                programs.map(|shader_sources| {
                    let program = gl.create_program().expect("Cannot create program");

                    let shaders: Vec<_> = shader_sources
                        .iter()
                        .map(|(shader_type, shader_source)| {
                            let shader = gl
                                .create_shader(*shader_type)
                                .expect("Cannot create shader");
                            
                            // For graph shaders, prepend common code
                            let full_source = if shader_source.contains("unpack_color") {
                                format!(
                                    "{shader_version}\n#define NUM_CLASSES {0}\n{common_glsl}\n{shader_source}",
                                    num_classes,
                                )
                            } else {
                                format!(
                                    "{shader_version}\n#define NUM_CLASSES {0}\n{shader_source}",
                                    num_classes,
                                )
                            };
                            
                            gl.shader_source(shader, &full_source);
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
                        "({} classes) {}",
                        num_classes,
                        gl.get_program_info_log(program)
                    );

                    for shader in shaders {
                        gl.detach_shader(program, shader);
                        gl.delete_shader(shader);
                    }

                    program
                })
            })?;

            log!(status_tx, t!("Creating node instance data"));
            
            // Create instance data for nodes (position, degree, class)
            let node_instances: Vec<NodeInstanceData> = viewer
                .persons
                .iter()
                .map(|p| NodeInstanceData {
                    position: p.position,
                    degree_and_class: ((p.modularity_class as u32) << 16) | (p.neighbors.len() as u32),
                })
                .collect();

            // Create instance data for edges with rendering cap
            
            log!(status_tx, t!("Computing edge lengths"));

            let mut edge_data = Vec::new();
            edges
                .into_par_iter()
                .map(|e| {
                    let pa = &viewer.persons[e.a as usize];
                    let pb = &viewer.persons[e.b as usize];
                    let dist = (pa.position - pb.position).norm_squared();
                    (pa, pb, dist, e)
                })
                .collect_into_vec(&mut edge_data);
            
            log!(status_tx, t!("Sorting edges by length"));

            edge_data.par_sort_unstable_by_key(|&(_, _, dist, _)| {
                // Reverse order
                std::cmp::Reverse(dist.to_bits())
            });
            
            log!(status_tx, t!("Creating edge instance data"));
            
            let edge_instances: Vec<EdgeInstanceData> = edge_data.into_iter()
                .map(|(pa, pb, _, _)| EdgeInstanceData {
                    position_a: pa.position,
                    position_b: pb.position,
                    degree_and_class_a: ((pa.modularity_class as u32) << 16) | (pa.neighbors.len() as u32),
                    degree_and_class_b: ((pb.modularity_class as u32) << 16) | (pb.neighbors.len() as u32),
                })
                .collect();

            let nodes_count = viewer.persons.len();
            let edges_count = edge_instances.len();

            log!(
                status_tx,
                t!(
                    "New node count: %{num}, edge count: %{edges}",
                    num = nodes_count,
                    edges = edges_count
                )
            );

            log!(status_tx, t!("Creating quad templates and buffers"));
            
            // Create templates
            let quad_template = geom_draw::create_quad_template();
            let edge_quad_template = geom_draw::create_edge_quad_template();
            
            let (quad_vao, quad_vbo, edge_quad_vao, edge_quad_vbo, node_instance_buffer, edge_instance_buffer) = 
                gl.run(move |gl: &glow::Context| {
                    // Create node quad VAO and VBO
                    let quad_vao = gl.create_vertex_array().expect("Cannot create quad VAO");
                    gl.bind_vertex_array(Some(quad_vao));
                    
                    let quad_vbo = gl.create_buffer().expect("Cannot create quad VBO");
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(quad_vbo));
                    gl.buffer_data_u8_slice(
                        glow::ARRAY_BUFFER,
                        std::slice::from_raw_parts(
                            quad_template.as_ptr() as *const u8,
                            size_of_val(&quad_template[..]),
                        ),
                        glow::STATIC_DRAW,
                    );
                    
                    // Vertex position (location = 0)
                    gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 
                        size_of::<QuadVertex>() as i32, 0);
                    gl.enable_vertex_attrib_array(0);
                    
                    // Texture coordinates (location = 1)
                    gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false,
                        size_of::<QuadVertex>() as i32,
                        size_of::<Point>() as i32);
                    gl.enable_vertex_attrib_array(1);
                    
                    // Create instance buffer
                    let instance_buffer = gl.create_buffer().expect("Cannot create instance buffer");
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(instance_buffer));
                    gl.buffer_data_size(
                        glow::ARRAY_BUFFER,
                        (nodes_count * size_of::<NodeInstanceData>()).try_into().unwrap(),
                        glow::STATIC_DRAW,
                    );
                    
                    // Instance position (location = 2)
                    gl.vertex_attrib_pointer_f32(2, 2, glow::FLOAT, false,
                        size_of::<NodeInstanceData>() as i32, 0);
                    gl.enable_vertex_attrib_array(2);
                    gl.vertex_attrib_divisor(2, 1);
                    
                    // Instance degree_and_class (location = 3)
                    gl.vertex_attrib_pointer_i32(3, 1, glow::UNSIGNED_INT,
                        size_of::<NodeInstanceData>() as i32,
                        size_of::<Point>() as i32);
                    gl.enable_vertex_attrib_array(3);
                    gl.vertex_attrib_divisor(3, 1);
                    
                    // Create edge quad VAO and VBO
                    let edge_quad_vao = gl.create_vertex_array().expect("Cannot create edge quad VAO");
                    gl.bind_vertex_array(Some(edge_quad_vao));
                    
                    let edge_quad_vbo = gl.create_buffer().expect("Cannot create edge quad VBO");
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(edge_quad_vbo));
                    gl.buffer_data_u8_slice(
                        glow::ARRAY_BUFFER,
                        std::slice::from_raw_parts(
                            edge_quad_template.as_ptr() as *const u8,
                            size_of_val(&edge_quad_template[..]),
                        ),
                        glow::STATIC_DRAW,
                    );
                    
                    // Vertex position (location = 0)
                    gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false,
                        size_of::<QuadVertex>() as i32, 0);
                    gl.enable_vertex_attrib_array(0);
                    
                    // Texture coordinates (location = 1)
                    gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false,
                        size_of::<QuadVertex>() as i32,
                        size_of::<Point>() as i32);
                    gl.enable_vertex_attrib_array(1);
                    
                    // Create edge instance buffer
                    let edge_instance_buffer = gl.create_buffer().expect("Cannot create edge instance buffer");
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(edge_instance_buffer));
                    gl.buffer_data_size(
                        glow::ARRAY_BUFFER,
                        (edges_count * size_of::<EdgeInstanceData>()).try_into().unwrap(),
                        glow::STATIC_DRAW,
                    );
                    
                    // Edge position A (location = 2)
                    gl.vertex_attrib_pointer_f32(2, 2, glow::FLOAT, false,
                        size_of::<EdgeInstanceData>() as i32, 0);
                    gl.enable_vertex_attrib_array(2);
                    gl.vertex_attrib_divisor(2, 1);
                    
                    // Edge position B (location = 3)
                    gl.vertex_attrib_pointer_f32(3, 2, glow::FLOAT, false,
                        size_of::<EdgeInstanceData>() as i32,
                        size_of::<Point>() as i32);
                    gl.enable_vertex_attrib_array(3);
                    gl.vertex_attrib_divisor(3, 1);
                    
                    // Edge degree_and_class A (location = 4)
                    gl.vertex_attrib_pointer_i32(4, 1, glow::UNSIGNED_INT,
                        size_of::<EdgeInstanceData>() as i32,
                        (2 * size_of::<Point>()) as i32);
                    gl.enable_vertex_attrib_array(4);
                    gl.vertex_attrib_divisor(4, 1);
                    
                    // Edge degree_and_class B (location = 5)
                    gl.vertex_attrib_pointer_i32(5, 1, glow::UNSIGNED_INT,
                        size_of::<EdgeInstanceData>() as i32,
                        (2 * size_of::<Point>() + size_of::<u32>()) as i32);
                    gl.enable_vertex_attrib_array(5);
                    gl.vertex_attrib_divisor(5, 1);

                    (quad_vao, quad_vbo, edge_quad_vao, edge_quad_vbo, instance_buffer, edge_instance_buffer)
                })?;

            log!(
                status_tx,
                t!("Buffering %{num} node instances", num = node_instances.len())
            );

            let node_instances = std::sync::Arc::new(node_instances);
            const BATCH_SIZE: usize = 1000000;

            for_progress!(status_tx, i in 0..node_instances.len().div_ceil(BATCH_SIZE), {
                let node_instances = node_instances.clone();
                gl.run(move |gl: &glow::Context| {
                    let start = i * BATCH_SIZE;
                    let end = ((i + 1) * BATCH_SIZE).min(node_instances.len());
                    let batch = &node_instances[start..end];
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(node_instance_buffer));
                    gl.buffer_sub_data_u8_slice(
                        glow::ARRAY_BUFFER,
                        (start * size_of::<NodeInstanceData>()).try_into().unwrap(),
                        std::slice::from_raw_parts(
                            batch.as_ptr() as *const u8,
                            size_of_val(batch),
                        ),
                    );
                })?;
            });

            log!(status_tx, t!("Buffering %{num} edge instances", num = edge_instances.len()));
            let edge_instances = std::sync::Arc::new(edge_instances);

            for_progress!(status_tx, i in 0..edge_instances.len().div_ceil(BATCH_SIZE), {
                let edge_instances = edge_instances.clone();
                gl.run(move |gl: &glow::Context| {
                    let start = i * BATCH_SIZE;
                    let end = ((i + 1) * BATCH_SIZE).min(edge_instances.len());
                    let batch = &edge_instances[start..end];
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(edge_instance_buffer));
                    gl.buffer_sub_data_u8_slice(
                        glow::ARRAY_BUFFER,
                        (start * size_of::<EdgeInstanceData>()).try_into().unwrap(),
                        std::slice::from_raw_parts(
                            batch.as_ptr() as *const u8,
                            size_of_val(batch),
                        ),
                    );
                })?;
            });

            log!(
                status_tx,
                t!(
                    "Done: %{time}",
                    time = chrono::Local::now().format("%H:%M:%S.%3f")
                )
            );

            Ok(Self {
                program_basic,
                program_edge,
                program_node,
                nodes_instance_buffer: node_instance_buffer,
                nodes_count,
                quad_vao,
                quad_vbo,
                edge_quad_vao,
                edge_quad_vbo,
                edge_instance_buffer,
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
            gl.delete_buffer(self.nodes_instance_buffer);
            gl.delete_buffer(self.quad_vbo);
            gl.delete_buffer(self.edge_quad_vbo);
            gl.delete_buffer(self.edge_instance_buffer);
            log::info!("Deleting arrays");
            gl.delete_vertex_array(self.quad_vao);
            gl.delete_vertex_array(self.edge_quad_vao);
        }
    }

    pub(crate) fn paint(
        &mut self,
        gl: &glow::Context,
        cam: Matrix4<f32>,
        edges: (bool, f32),
        nodes: (bool, f32),
        class_colors: &[u32],
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

            // Draw edges with instanced rendering
            if edges.0 {
                gl.bind_vertex_array(Some(self.edge_quad_vao));
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
                    ((self.node_filter.degree_filter.1 as u32) << 16)
                        | (self.node_filter.degree_filter.0 as u32),
                );
                gl.uniform_1_f32(
                    Some(
                        &gl.get_uniform_location(self.program_edge, "opacity")
                            .unwrap(),
                    ),
                    edges.1,
                );

                gl.uniform_1_u32_slice(
                    Some(
                        &gl.get_uniform_location(self.program_edge, "u_class_colors")
                            .unwrap(),
                    ),
                    &class_colors,
                );
                
                let instances = self.edges_count as i32;
                #[cfg(target_arch = "wasm32")]
                let instances = instances.min(5_000_000); // Limit for Firefox
                gl.draw_arrays_instanced(glow::TRIANGLES, 0, 6, instances);
            }
            
            // Draw nodes with instanced rendering
            if nodes.0 {
                gl.bind_vertex_array(Some(self.quad_vao));
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
                        ((self.node_filter.degree_filter.1 as u32) << 16)
                            | (self.node_filter.degree_filter.0 as u32)
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

                gl.uniform_1_u32_slice(
                    Some(
                        &gl.get_uniform_location(self.program_node, "u_class_colors")
                            .unwrap(),
                    ),
                    &class_colors,
                );
                gl.draw_arrays_instanced(glow::TRIANGLES, 0, 6, self.nodes_count as i32);
            }
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct QuadVertex {
    pub position: Point,
    pub tex_coord: Point,
}

impl QuadVertex {
    pub fn new(position: Point, tex_coord: Point) -> QuadVertex {
        QuadVertex {
            position,
            tex_coord,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct NodeInstanceData {
    pub position: Point,
    pub degree_and_class: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct EdgeInstanceData {
    pub position_a: Point,
    pub position_b: Point,
    pub degree_and_class_a: u32,
    pub degree_and_class_b: u32,
}
