use crate::algorithms::AbstractGraph;
use crate::app::{Person, ViewerData};
use crate::graph_render::{GlTask, NodeFilter, PersonVertex, RenderedGraph};
use crate::threading::MyRwLock;
use eframe::glow;
use eframe::glow::HasContext;
use egui::{Color32, Id, Ui};
use itertools::Itertools;
use modal::ModalWriter;
use std::ops::RangeInclusive;
use std::sync::Arc;

pub(crate) mod modal;
pub mod sections;
pub(crate) mod tabs;
mod widgets;

use sections::*;
use tabs::{NewTabRequest, TabCamera};

fn set_bg_color_tinted(base: Color32, ui: &mut Ui) {
    let visuals = &mut ui.style_mut().visuals;

    const MIX: f32 = 1.0 / 3.0;

    fn mix(orig: u8, tint: u8) -> u8 {
        (orig as f32 * (1.0 - MIX * (1.0 - (tint as f32) / 255.0))) as u8
    }

    for orig in [
        &mut visuals.widgets.inactive.weak_bg_fill,
        &mut visuals.widgets.hovered.weak_bg_fill,
        &mut visuals.widgets.active.weak_bg_fill,
        &mut visuals.widgets.open.weak_bg_fill,
    ] {
        let [r, g, b, a] = orig.to_array();
        *orig = Color32::from_rgba_unmultiplied(
            mix(r, base.r()),
            mix(g, base.g()),
            mix(b, base.b()),
            a,
        );
    }
}

#[derive(Default)]
struct ParadoxState {
    current: Option<usize>,
    sum: usize,
    min: usize,
    max: usize,
}

fn rerender_graph(persons: &[Person]) -> GlTask {
    use crate::graph_render::{NodeInstanceData, EdgeInstanceData};
    
    let node_instances: Vec<NodeInstanceData> = persons
        .iter()
        .map(|p| NodeInstanceData {
            position: p.position,
            degree_and_class: ((p.modularity_class as u32) << 16) | (p.neighbors.len() as u32),
        })
        .collect();

    let edge_instances: Vec<EdgeInstanceData> = persons.iter().get_edges().map(|(a, b)| {
        let pa = &persons[a];
        let pb = &persons[b];
        EdgeInstanceData {
            position_a: pa.position,
            position_b: pb.position,
            degree_and_class_a: ((pa.modularity_class as u32) << 16) | (pa.neighbors.len() as u32),
            degree_and_class_b: ((pb.modularity_class as u32) << 16) | (pb.neighbors.len() as u32),
        }
    }).collect();

    let closure = move |graph: &mut RenderedGraph, gl: &glow::Context| unsafe {
        // Update instance buffer for nodes
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(graph.nodes_instance_buffer));
        gl.buffer_sub_data_u8_slice(
            glow::ARRAY_BUFFER,
            0,
            std::slice::from_raw_parts(
                node_instances.as_ptr() as *const u8,
                node_instances.len() * size_of::<NodeInstanceData>(),
            ),
        );
        
        // Update edge instance buffer
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(graph.edge_instance_buffer));
        gl.buffer_sub_data_u8_slice(
            glow::ARRAY_BUFFER,
            0,
            std::slice::from_raw_parts(
                edge_instances.as_ptr() as *const u8,
                edge_instances.len() * size_of::<EdgeInstanceData>(),
            ),
        );
    };

    Box::new(closure)
}

#[derive(Default, PartialEq, Eq)]
pub enum SelectedUserField {
    Selected,
    #[default]
    PathSource,
    PathDest,
}

#[derive(Default)]
pub struct NodeStats {
    node_count: usize,
    node_classes: Vec<(usize, usize)>,
}

impl NodeStats {
    pub fn new(data: &ViewerData, filter: NodeFilter) -> Self {
        let mut count_classes = vec![0; data.modularity_classes.len()];
        let mut node_count = 0;
        for p in &*data.persons {
            let ok = if filter.filter_nodes {
                let deg = p.neighbors.len() as u16;
                deg >= filter.degree_filter.0 && deg <= filter.degree_filter.1
            } else {
                true
            };
            if ok {
                node_count += 1;
                count_classes[p.modularity_class as usize] += 1;
            }
        }
        let node_classes = count_classes
            .iter()
            .enumerate()
            .filter(|(_, &c)| c != 0)
            .sorted_by_key(|(_, &c)| std::cmp::Reverse(c))
            .map(|(i, &c)| (i, c))
            .collect_vec();
        Self {
            node_count,
            node_classes,
        }
    }
}

#[derive(Default)]
pub struct UiState {
    pub display: display::DisplaySection,
    pub path: path::PathSection,
    pub classes: class::ClassSection,
    pub infos: infos::InfosSection,
    pub details: details::DetailsSection,
    pub selected_user_field: SelectedUserField,
    pub algorithms: algos::AlgosSection,

    pub stats: Arc<MyRwLock<NodeStats>>,
}

fn percent_formatter(val: f64, _: RangeInclusive<usize>) -> String {
    format!("{:.1}%", val * 100.0)
}

fn percent_parser(s: &str) -> Option<f64> {
    s.strip_suffix('%')
        .unwrap_or(s)
        .parse()
        .ok()
        .map(|v: f64| v / 100.0)
}

impl UiState {
    pub fn draw_ui(
        &mut self,
        ui: &mut Ui,
        data: &Arc<MyRwLock<ViewerData>>,
        graph: &Arc<MyRwLock<RenderedGraph>>,
        tab_request: &mut Option<NewTabRequest>,
        camera: &mut TabCamera,
        cid: Id,
        modal: &impl ModalWriter,
    ) {
        ui.spacing_mut().slider_width = 200.0;
        egui::ScrollArea::vertical()
            .max_width(ui.ctx().screen_rect().width() / 3.0)
            .show(ui, |ui| {
                self.display.show(graph, ui, &self.stats);

                if self.display.deg_filter_changed {
                    *self.stats.write() = NodeStats::new(&data.read(), graph.read().node_filter);
                }

                self.path
                    .show(data, ui, &mut self.infos, &mut self.selected_user_field);

                self.infos.show(
                    data,
                    tab_request,
                    ui,
                    &camera.camera,
                    &self.path,
                    &mut self.selected_user_field,
                    modal,
                );

                self.classes.show(
                    ui,
                    &self.infos,
                    data,
                    tab_request,
                    &camera.camera,
                    &self.path,
                    modal,
                    &self.stats,
                );

                self.algorithms.show(data, ui, graph, &self.stats, modal);

                self.details.show(ui, camera, cid);
            });
    }
}
