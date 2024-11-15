use crate::graph_render::RenderedGraph;
use crate::threading::MyRwLock;
use crate::ui;
use derivative::Derivative;
use egui::{CollapsingHeader, SliderClamping, Ui};
use std::sync::Arc;

#[derive(Derivative)]
#[derivative(Default)]
pub struct DisplaySection {
    #[derivative(Default(value = "true"))]
    pub g_show_nodes: bool,
    //#[derivative(Default(value = "cfg!(not(target_arch = \"wasm32\"))"))]
    #[derivative(Default(value = "true"))]
    pub g_show_edges: bool,
    pub g_opac_nodes: f32,
    pub g_opac_edges: f32,
    pub deg_filter_changed: bool,
    pub max_degree: u16,
    pub node_count: usize,
}

impl DisplaySection {
    pub(crate) fn show(&mut self, graph: &Arc<MyRwLock<RenderedGraph>>, ui: &mut Ui) {
        CollapsingHeader::new(t!("Display"))
            .default_open(true)
            .show(ui, |ui| {
                ui.checkbox(&mut self.g_show_nodes, t!("Show nodes"));
                if self.g_show_nodes {
                    ui.add(
                        egui::Slider::new(&mut self.g_opac_nodes, 0.0..=1.0)
                            .text(t!("Opacity"))
                            .custom_formatter(ui::percent_formatter)
                            .custom_parser(ui::percent_parser)
                            .clamping(SliderClamping::Always),
                    );
                }
                ui.checkbox(&mut self.g_show_edges, t!("Show links"));
                if self.g_show_edges {
                    ui.add(
                        egui::Slider::new(&mut self.g_opac_edges, 0.0..=1.0)
                            .text(t!("Opacity"))
                            .custom_formatter(ui::percent_formatter)
                            .custom_parser(ui::percent_parser)
                            .clamping(SliderClamping::Always),
                    );
                }

                ui.horizontal(|ui| {
                    let mut graph_lock = graph.write();
                    let graph = &mut *graph_lock;
                    ui.vertical(|ui| {
                        let start = ui
                            .add(
                                egui::DragValue::new(&mut graph.node_filter.degree_filter.0)
                                    .speed(1)
                                    .range(1..=graph.node_filter.degree_filter.1)
                                    .prefix(t!("Minimum degree: ")),
                            )
                            .changed();
                        let end = ui
                            .add(
                                egui::DragValue::new(&mut graph.node_filter.degree_filter.1)
                                    .speed(1)
                                    .range(graph.node_filter.degree_filter.0..=self.max_degree)
                                    .prefix(t!("Maximum degree: ")),
                            )
                            .changed();
                        if start || end {
                            self.deg_filter_changed = true;
                        }
                    });
                    ui.vertical(|ui| {
                        ui.checkbox(&mut graph.node_filter.filter_nodes, t!("Filter nodes"));
                    });
                });

                ui.horizontal(|ui| {
                    ui.label(t!("Visible nodes: "));
                    ui.label(format!("{}", self.node_count));
                });
            });
    }
}