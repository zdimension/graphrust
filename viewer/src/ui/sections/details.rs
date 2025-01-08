use crate::ui::tabs::{CamAnimating, TabCamera};
use derivative::Derivative;
use eframe::emath::Pos2;
use egui::{CollapsingHeader, Id, Ui};
use graph_format::nalgebra::Vector2;

#[derive(Derivative)]
#[derivative(Default)]
pub struct DetailsSection {
    pub mouse_pos: Option<Pos2>,
    pub mouse_pos_world: Option<Vector2<f32>>,
}

impl DetailsSection {
    pub(crate) fn show(&mut self, ui: &mut Ui, camera: &mut TabCamera, cid: Id) {
        CollapsingHeader::new(t!("Details"))
            .id_salt("details")
            .default_open(false)
            .show(ui, |ui| {
                let trans = &camera.camera.transf;
                egui::Grid::new("#mouse_pos").show(ui, |ui| {
                    ui.label(t!("Position:"));
                    ui.label(format!(
                        "{:?}",
                        self.mouse_pos.map(|p| Vector2::new(p.x, p.y))
                    ));
                    ui.end_row();
                    ui.label(t!("Position (world):"));
                    ui.label(format!("{:?}", self.mouse_pos_world));
                    ui.end_row();
                    ui.label(t!("Scale:"));
                    ui.label(format!("{:.3}", trans.scaling()));
                    ui.end_row();
                    ui.label(t!("Angle:"));
                    ui.label(format!("{:.3}", trans.isometry.rotation.angle()));
                    ui.end_row();
                    ui.label(t!("Translation:"));
                    let offs = trans.isometry.translation;
                    ui.label(format!("({:.3}, {:.3})", offs.x, offs.y));
                    ui.end_row();
                });
                if ui.button(t!("Reset camera")).clicked() {
                    camera.camera = camera.camera_default;
                }
                if ui.button(t!("Center camera")).clicked() {
                    ui.ctx().animate_bool_with_time(cid, true, 0.0);
                    camera.cam_animating = Some(CamAnimating::PanTo {
                        from: camera.camera.transf,
                        to: camera.camera_default.transf,
                    });
                }

                let matrix = camera.camera.get_matrix();
                egui::Grid::new("#cammatrix").show(ui, move |ui| {
                    for i in 0..4 {
                        for j in 0..4 {
                            // format fixed width
                            ui.label(format!("{:.3}", matrix[(i, j)]));
                        }
                        ui.end_row();
                    }
                });
            });
    }
}
