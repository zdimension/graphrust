use crate::app::{ModularityClass, ViewerData};
use crate::graph_render::camera::Camera;
use crate::threading::MyRwLock;
use crate::ui::infos::InfosSection;
use crate::ui::modal::ModalWriter;
use crate::ui::path::PathSection;
use crate::ui::tabs::NewTabRequest;
use crate::ui::NodeStats;
use eframe::emath::Vec2;
use eframe::epaint::Color32;
use egui::{CollapsingHeader, Sense, Ui};
use egui_extras::{Column, TableBuilder};
use graph_format::Color3b;
use std::sync::Arc;

#[derive(Default)]
pub struct ClassSection {}

impl ClassSection {
    pub(crate) fn show(
        &mut self,
        ui: &mut Ui,
        infos_section: &InfosSection,
        data_rw: &Arc<MyRwLock<ViewerData>>,
        tab_request: &mut Option<NewTabRequest>,
        camera: &Camera,
        path_section: &PathSection,
        modal: &impl ModalWriter,
        stats: &Arc<MyRwLock<NodeStats>>,
    ) {
        CollapsingHeader::new(t!("Classes (%{num})", num = stats.read().node_classes.len()))
            .id_salt("classes")
            .default_open(false)
            .show(ui, |ui| {
                TableBuilder::new(ui)
                    .column(Column::exact(20.0))
                    .column(Column::exact(40.0))
                    .column(Column::exact(70.0))
                    .body(|mut body| {
                        let data = data_rw.read();
                        for &(clid, count) in &stats.read().node_classes {
                            body.row(15.0, |mut row| {
                                let cl = &data.modularity_classes[clid];
                                row.col(|ui| {
                                    Self::class_circle(ui, cl);
                                });
                                row.col(|ui| {
                                    // ui.label(format!("{}", cl.id));
                                    InfosSection::create_class_subgraph(
                                        infos_section,
                                        data_rw,
                                        tab_request,
                                        camera,
                                        path_section,
                                        modal,
                                        clid.try_into().unwrap(),
                                        ui,
                                    );
                                });
                                row.col(|ui| {
                                    ui.label(format!("{}", count));
                                });
                            });
                        }
                    });
            });
    }

    pub(crate) fn class_circle(ui: &mut Ui, cl: &ModularityClass) {
        let rad = 5.0;
        let size = Vec2::splat(2.0 * rad + 5.0);
        let (rect, _) = ui.allocate_at_least(size, Sense::hover());
        let Color3b { r, g, b } = cl.color;
        ui.painter().circle_filled(
            rect.center(),
            rad,
            Color32::from_rgb(r / 2, g / 2, b / 2),
        );
    }
}