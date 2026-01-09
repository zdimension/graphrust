use crate::app::ViewerData;
use crate::threading::{Cancelable, StatusWriter};
use crate::{for_progress, log};
use anyhow::anyhow;
use derivative::Derivative;
use eframe::wgpu;
use eframe::wgpu::util::DeviceExt;
use graph_format::nalgebra::Matrix4;
use graph_format::{EdgeStore, Point};
use std::collections::VecDeque;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use rayon::prelude::*;

pub mod camera;
pub mod geom_draw;

pub type WgpuWorkResult = Box<dyn std::any::Any + Send>;

pub struct WgpuWork(pub(crate) Box<dyn Send + FnOnce(&wgpu::Device, &wgpu::Queue, &Sender<WgpuWorkResult>)>);

pub type WgpuMpsc = (Receiver<WgpuWork>, Sender<WgpuWorkResult>);

/// A forwarder for sending work to the WGPU thread
///
/// This is a simple wrapper around an MPSC channel that allows sending work to the WGPU thread.
/// Internally, it sends a boxed closure that is run on the next frame, and returns the result
/// inside a Box<dyn Any>.
pub struct WgpuForwarder {
    tx: Sender<WgpuWork>,
    rx: Receiver<WgpuWorkResult>,
}

impl WgpuForwarder {
    pub fn new() -> (WgpuForwarder, WgpuMpsc) {
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

    pub fn run<R: Send + 'static, T: FnOnce(&wgpu::Device, &wgpu::Queue) -> R + Send + 'static>(
        &self,
        work: T,
    ) -> Cancelable<R> {
        self.tx.send(WgpuWork(Box::new(move |device, queue, tx| {
            tx.send(Box::new(work(device, queue))).unwrap();
        })))?;
        Ok(*self
            .rx
            .recv()?
            .downcast()
            .map_err(|_| anyhow!("Failed to downcast"))?)
    }
}

pub type WgpuTask = Box<dyn FnOnce(&mut RenderedGraph, &wgpu::Device, &wgpu::Queue) + Send + Sync + 'static>;

#[derive(Copy, Clone, Derivative)]
#[derivative(Default())]
pub struct NodeFilter {
    #[derivative(Default(value = "(0, u16::MAX)"))]
    pub degree_filter: (u16, u16),
    pub filter_nodes: bool,
}

pub struct RenderedGraph {
    pub node_pipeline: wgpu::RenderPipeline,
    pub edge_pipeline: wgpu::RenderPipeline,
    pub node_bind_group: wgpu::BindGroup,
    pub edge_bind_group: wgpu::BindGroup,
    pub node_uniform_buffer: wgpu::Buffer,
    pub edge_uniform_buffer: wgpu::Buffer,
    pub class_colors_buffer: wgpu::Buffer,
    pub nodes_instance_buffer: wgpu::Buffer,
    pub nodes_count: usize,
    pub quad_vertex_buffer: wgpu::Buffer,
    pub edge_quad_vertex_buffer: wgpu::Buffer,
    pub edge_instance_buffer: wgpu::Buffer,
    pub edges_count: usize,
    pub node_filter: NodeFilter,
    pub destroyed: bool,
    pub tasks: VecDeque<WgpuTask>,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct NodeUniforms {
    projection: [[f32; 4]; 4],
    degfilter: u32,
    opacity: f32,
    _padding: [u32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct EdgeUniforms {
    projection: [[f32; 4]; 4],
    degfilter: u32,
    opacity: f32,
    _padding: [u32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct BasicUniforms {
    projection: [[f32; 4]; 4],
}

impl RenderedGraph {
    pub fn new<'a>(
        wgpu: WgpuForwarder,
        viewer: &ViewerData,
        edges: Vec<EdgeStore>,
        status_tx: StatusWriter,
    ) -> Cancelable<Self> {
        use graph_format::Point;
        use std::collections::VecDeque;

        log!(status_tx, t!("Creating shader modules"));
        
        let _num_classes = viewer.modularity_classes.len();
        
        // Load shaders
        let node_shader_src = include_str!("shaders/graph_node.wgsl");
        let edge_shader_src = include_str!("shaders/graph_edge.wgsl");

        #[cfg(target_arch = "wasm32")]
        let edges = edges.take(10_000_000);

        let _edges_count = edges.len();
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
        #[cfg(target_arch = "wasm32")]
        const MAX_RENDERED_EDGES: usize = 500_000;
        #[cfg(not(target_arch = "wasm32"))]
        const MAX_RENDERED_EDGES: usize = 5_000_000;
        
        log!(status_tx, t!("Sorting edges data (max %{num})", num = MAX_RENDERED_EDGES));

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

        edge_data.par_sort_unstable_by_key(|&(_, _, dist, _)| {
            // Reverse order
            std::cmp::Reverse(dist.to_bits())
        });
        
        log!(status_tx, t!("Creating edge instance data"));
        
        let edge_instances: Vec<EdgeInstanceData> = edge_data.into_iter()
            .take(MAX_RENDERED_EDGES)
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
        
        let class_colors: Vec<u32> = viewer
            .modularity_classes
            .iter()
            .map(|c| c.color.to_u32())
            .collect();

        let result = wgpu.run(move |device: &wgpu::Device, _queue: &wgpu::Queue| {
            // Create shader modules
            let node_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Node Shader"),
                source: wgpu::ShaderSource::Wgsl(node_shader_src.into()),
            });
            
            let edge_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Edge Shader"),
                source: wgpu::ShaderSource::Wgsl(edge_shader_src.into()),
            });

            // Create vertex buffers
            let quad_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Node Quad Vertex Buffer"),
                contents: bytemuck::cast_slice(&quad_template),
                usage: wgpu::BufferUsages::VERTEX,
            });

            let edge_quad_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Edge Quad Vertex Buffer"),
                contents: bytemuck::cast_slice(&edge_quad_template),
                usage: wgpu::BufferUsages::VERTEX,
            });

            // Create instance buffers (will be filled later)
            let nodes_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Node Instance Buffer"),
                size: (nodes_count * std::mem::size_of::<NodeInstanceData>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let edge_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Edge Instance Buffer"),
                size: (edges_count * std::mem::size_of::<EdgeInstanceData>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            // Create uniform buffers
            let node_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Node Uniform Buffer"),
                size: std::mem::size_of::<NodeUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let edge_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Edge Uniform Buffer"),
                size: std::mem::size_of::<EdgeUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            // Create storage buffer for class colors
            let class_colors_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Class Colors Buffer"),
                contents: bytemuck::cast_slice(&class_colors),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });

            // Create bind group layouts
            let node_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Node Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

            let edge_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Edge Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });


            // Create bind groups
            let node_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Node Bind Group"),
                layout: &node_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: node_uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: class_colors_buffer.as_entire_binding(),
                    },
                ],
            });

            let edge_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Edge Bind Group"),
                layout: &edge_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: edge_uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: class_colors_buffer.as_entire_binding(),
                    },
                ],
            });

            // Define vertex buffer layouts
            let quad_vertex_layout = wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<QuadVertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                    wgpu::VertexAttribute {
                        offset: std::mem::size_of::<Point>() as wgpu::BufferAddress,
                        shader_location: 1,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                ],
            };
            
            let quad_vertex_layout_2 = wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<QuadVertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                    wgpu::VertexAttribute {
                        offset: std::mem::size_of::<Point>() as wgpu::BufferAddress,
                        shader_location: 1,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                ],
            };

            let node_instance_layout = wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<NodeInstanceData>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &[
                    wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 2,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                    wgpu::VertexAttribute {
                        offset: std::mem::size_of::<Point>() as wgpu::BufferAddress,
                        shader_location: 3,
                        format: wgpu::VertexFormat::Uint32,
                    },
                ],
            };

            let edge_instance_layout = wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<EdgeInstanceData>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &[
                    wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 2,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                    wgpu::VertexAttribute {
                        offset: std::mem::size_of::<Point>() as wgpu::BufferAddress,
                        shader_location: 3,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                    wgpu::VertexAttribute {
                        offset: (2 * std::mem::size_of::<Point>()) as wgpu::BufferAddress,
                        shader_location: 4,
                        format: wgpu::VertexFormat::Uint32,
                    },
                    wgpu::VertexAttribute {
                        offset: (2 * std::mem::size_of::<Point>() + std::mem::size_of::<u32>()) as wgpu::BufferAddress,
                        shader_location: 5,
                        format: wgpu::VertexFormat::Uint32,
                    },
                ],
            };

            // Create pipeline layouts
            let node_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Node Pipeline Layout"),
                bind_group_layouts: &[&node_bind_group_layout],
                push_constant_ranges: &[],
            });

            let edge_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Edge Pipeline Layout"),
                bind_group_layouts: &[&edge_bind_group_layout],
                push_constant_ranges: &[],
            });

            // Create render pipelines
            let node_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Node Render Pipeline"),
                layout: Some(&node_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &node_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[quad_vertex_layout, node_instance_layout],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &node_shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Bgra8Unorm,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
                cache: None,
            });

            let edge_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Edge Render Pipeline"),
                layout: Some(&edge_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &edge_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[quad_vertex_layout_2, edge_instance_layout],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &edge_shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Bgra8Unorm,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
                cache: None,
            });

            (
                node_pipeline,
                edge_pipeline,
                node_bind_group,
                edge_bind_group,
                node_uniform_buffer,
                edge_uniform_buffer,
                class_colors_buffer,
                nodes_instance_buffer,
                quad_vertex_buffer,
                edge_quad_vertex_buffer,
                edge_instance_buffer,
            )
        })?;

        let (
            node_pipeline,
            edge_pipeline,
            node_bind_group,
            edge_bind_group,
            node_uniform_buffer,
            edge_uniform_buffer,
            class_colors_buffer,
            nodes_instance_buffer,
            quad_vertex_buffer,
            edge_quad_vertex_buffer,
            edge_instance_buffer,
        ) = result;

        log!(
            status_tx,
            t!("Buffering %{num} node instances", num = node_instances.len())
        );

        let node_instances = std::sync::Arc::new(node_instances);
        const BATCH_SIZE: usize = 1000000;

        let nodes_buffer_arc = std::sync::Arc::new(nodes_instance_buffer);
        for_progress!(status_tx, i in 0..node_instances.len().div_ceil(BATCH_SIZE), {
            let node_instances = node_instances.clone();
            let nodes_buffer = nodes_buffer_arc.clone();
            wgpu.run(move |_device: &wgpu::Device, queue: &wgpu::Queue| {
                let start = i * BATCH_SIZE;
                let end = ((i + 1) * BATCH_SIZE).min(node_instances.len());
                let batch = &node_instances[start..end];
                queue.write_buffer(
                    &nodes_buffer,
                    (start * std::mem::size_of::<NodeInstanceData>()) as u64,
                    bytemuck::cast_slice(batch),
                );
            })?
        });
        let nodes_instance_buffer = std::sync::Arc::try_unwrap(nodes_buffer_arc)
            .expect("Failed to unwrap nodes buffer Arc");

        log!(status_tx, t!("Buffering %{num} edge instances", num = edge_instances.len()));
        let edge_instances = std::sync::Arc::new(edge_instances);

        let edge_buffer_arc = std::sync::Arc::new(edge_instance_buffer);
        for_progress!(status_tx, i in 0..edge_instances.len().div_ceil(BATCH_SIZE), {
            let edge_instances = edge_instances.clone();
            let edge_buffer = edge_buffer_arc.clone();
            wgpu.run(move |_device: &wgpu::Device, queue: &wgpu::Queue| {
                let start = i * BATCH_SIZE;
                let end = ((i + 1) * BATCH_SIZE).min(edge_instances.len());
                let batch = &edge_instances[start..end];
                queue.write_buffer(
                    &edge_buffer,
                    (start * std::mem::size_of::<EdgeInstanceData>()) as u64,
                    bytemuck::cast_slice(batch),
                );
            })?
        });
        let edge_instance_buffer = std::sync::Arc::try_unwrap(edge_buffer_arc)
            .expect("Failed to unwrap edge buffer Arc");

        log!(
            status_tx,
            t!(
                "Done: %{time}",
                time = chrono::Local::now().format("%H:%M:%S.%3f")
            )
        );

        Ok(Self {
            node_pipeline,
            edge_pipeline,
            node_bind_group,
            edge_bind_group,
            node_uniform_buffer,
            edge_uniform_buffer,
            class_colors_buffer,
            nodes_instance_buffer,
            nodes_count,
            quad_vertex_buffer,
            edge_quad_vertex_buffer,
            edge_instance_buffer,
            edges_count,
            node_filter: NodeFilter::default(),
            destroyed: false,
            tasks: VecDeque::new(),
        })
    }

    pub(crate) fn destroy(&mut self) {
        log::info!("Destroying graph");
        self.destroyed = true;
        // In wgpu, resources are automatically cleaned up when dropped
        // No explicit cleanup needed like in OpenGL
    }

    pub(crate) fn paint(
        &mut self,
        render_pass: &mut eframe::wgpu::RenderPass<'_>,
        _cam: Matrix4<f32>,
        edges: (bool, f32),
        nodes: (bool, f32),
        _degfilter: u32,
        _node_degfilter: u32,
    ) {
        if self.destroyed {
            return;
        }

        // Draw edges with instanced rendering
        if edges.0 {
            render_pass.set_pipeline(&self.edge_pipeline);
            render_pass.set_bind_group(0, &self.edge_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.edge_quad_vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, self.edge_instance_buffer.slice(..));
            
            let instances = self.edges_count as u32;
            #[cfg(target_arch = "wasm32")]
            let instances = instances.min(5_000_000); // Limit for Firefox
            render_pass.draw(0..6, 0..instances);
        }

        // Draw nodes with instanced rendering
        if nodes.0 {
            render_pass.set_pipeline(&self.node_pipeline);
            render_pass.set_bind_group(0, &self.node_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, self.nodes_instance_buffer.slice(..));
            render_pass.draw(0..6, 0..self.nodes_count as u32);
        }
    }
    
    pub(crate) fn update_uniforms(
        &mut self,
        queue: &eframe::wgpu::Queue,
        cam: Matrix4<f32>,
        edges: (bool, f32),
        nodes: (bool, f32),
    ) {
        // Update edge uniforms
        let edge_uniforms = EdgeUniforms {
            projection: cam.into(),
            degfilter: ((self.node_filter.degree_filter.1 as u32) << 16)
                | (self.node_filter.degree_filter.0 as u32),
            opacity: edges.1,
            _padding: [0; 2],
        };
        queue.write_buffer(&self.edge_uniform_buffer, 0, bytemuck::bytes_of(&edge_uniforms));

        // Update node uniforms
        let node_uniforms = NodeUniforms {
            projection: cam.into(),
            degfilter: if self.node_filter.filter_nodes {
                ((self.node_filter.degree_filter.1 as u32) << 16)
                    | (self.node_filter.degree_filter.0 as u32)
            } else {
                0xffff_0000
            },
            opacity: nodes.1,
            _padding: [0; 2],
        };
        queue.write_buffer(&self.node_uniform_buffer, 0, bytemuck::bytes_of(&node_uniforms));
    }
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
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
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct NodeInstanceData {
    pub position: Point,
    pub degree_and_class: u32,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EdgeInstanceData {
    pub position_a: Point,
    pub position_b: Point,
    pub degree_and_class_a: u32,
    pub degree_and_class_b: u32,
}
