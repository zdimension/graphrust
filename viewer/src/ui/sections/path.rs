use crate::algorithms::pathfinding::{do_pathfinding, PathSectionResults, PathSectionSettings};
use crate::algorithms::AbstractNode;
use crate::app::ViewerData;
use crate::thread;
use crate::thread::JoinHandle;
use crate::threading::MyRwLock;
use crate::ui::infos::InfosSection;
use crate::ui::widgets::combo_filter::{combo_with_filter, COMBO_WIDTH};
use crate::ui::SelectedUserField;
use ahash::AHashSet;
use derivative::Derivative;
use eframe::emath::vec2;
use egui::{CollapsingHeader, Ui};
use itertools::Itertools;
use std::collections::VecDeque;
use std::sync::Arc;

#[derive(Derivative)]
#[derivative(Default)]
pub struct PathSection {
    pub path_settings: PathSectionSettings,
    pub path_dirty: bool,
    pub path_status: Option<PathStatus>,
    pub path_thread: Option<JoinHandle<Option<PathSectionResults>>>,
}

#[derive(Default)]
pub enum PathStatus {
    #[default]
    SameSrcDest,
    Loading,
    NoPath,
    PathFound(Vec<usize>),
}

impl PathSection {
    fn person_button(
        &self,
        data: &ViewerData,
        ui: &mut Ui,
        id: &usize,
        selected: &mut Option<usize>,
    ) {
        if ui
            .add(egui::Button::new(data.persons[*id].name).min_size(vec2(COMBO_WIDTH, 0.0)))
            .clicked()
        {
            *selected = Some(*id);
        }
    }

    pub(crate) fn show(
        &mut self,
        data: &Arc<MyRwLock<ViewerData>>,
        ui: &mut Ui,
        infos: &mut InfosSection,
        sel_field: &mut SelectedUserField,
    ) {
        if let Some(thr) = self.path_thread.take_if(|thr| thr.is_finished()) {
            let res = thr.join();
            self.path_thread = None;
            if let Ok(Some(res)) = res {
                self.path_status = Some(PathStatus::PathFound(res.path));
            } else {
                self.path_status = Some(PathStatus::NoPath);
            }
        }

        CollapsingHeader::new(t!("Shortest path"))
            .default_open(true)
            .show(ui, |ui| {
                let c1 = ui
                    .horizontal(|ui| {
                        ui.radio_value(sel_field, SelectedUserField::PathSource, "");
                        let c = combo_with_filter(
                            ui,
                            "#path_src",
                            &mut self.path_settings.path_src,
                            data,
                        );
                        if c.changed() {
                            infos.set_infos_current(self.path_settings.path_src);
                        }
                        if ui.button("✖").clicked() {
                            self.path_settings.path_src = None;
                        }
                        c
                    })
                    .inner;

                let c2 = ui
                    .horizontal(|ui| {
                        ui.radio_value(sel_field, SelectedUserField::PathDest, "");
                        let c = combo_with_filter(
                            ui,
                            "#path_dest",
                            &mut self.path_settings.path_dest,
                            data,
                        );
                        if c.changed() {
                            infos.set_infos_current(self.path_settings.path_dest);
                        }
                        if ui.button("✖").clicked() {
                            self.path_settings.path_dest = None;
                        }
                        c
                    })
                    .inner;

                ui.horizontal(|ui| {
                    ui.label(t!("Excluded:"));
                    if ui
                        .button("✖")
                        .on_hover_text(t!("Clear the exclusion list"))
                        .clicked()
                    {
                        self.path_settings.exclude_ids.clear();
                        self.path_dirty = true;
                    }
                });

                {
                    let mut cur_excl = None;
                    let mut del_excl = None;
                    let data = data.read();
                    for (i, id) in self.path_settings.exclude_ids.iter().enumerate() {
                        ui.horizontal(|ui| {
                            self.person_button(&data, ui, id, &mut cur_excl);
                            if ui
                                .button("✖")
                                .on_hover_text(t!("Remove from the exclusion list"))
                                .clicked()
                            {
                                del_excl = Some(i);
                            }
                        });
                    }
                    if let Some(id) = cur_excl {
                        infos.set_infos_current(Some(id));
                    }
                    if let Some(i) = del_excl {
                        self.path_dirty = true;
                        self.path_settings.exclude_ids.remove(i);
                    }
                }

                if (self.path_dirty || c1.changed() || c2.changed())
                    | ui.checkbox(
                        &mut self.path_settings.path_no_direct,
                        t!("Avoid direct link"),
                    )
                    .changed()
                    | ui.checkbox(
                        &mut self.path_settings.path_no_mutual,
                        t!("Avoid mutual friends"),
                    )
                    .changed()
                {
                    self.path_dirty = false;
                    self.path_status =
                        match (self.path_settings.path_src, self.path_settings.path_dest) {
                            (Some(x), Some(y)) if x == y => Some(PathStatus::SameSrcDest),
                            (None, _) | (_, None) => None,
                            _ => {
                                log::info!("Starting pathfinding");
                                let settings = self.path_settings.clone();
                                let data = data.clone();
                                self.path_thread = Some(thread::spawn(move || {
                                    let start = chrono::Utc::now();
                                    let data = data.read().persons.clone();
                                    let res = do_pathfinding(settings, &data);
                                    log::info!("Pathfinding took {:?}", chrono::Utc::now() - start);
                                    res
                                }));
                                Some(PathStatus::Loading)
                            }
                        }
                }

                if let Some(st) = &self.path_status {
                    use crate::ui;
                    use eframe::epaint::Color32;
                    use PathStatus::*;
                    match st {
                        SameSrcDest => {
                            ui.label(t!("🚫 Source and destination are the same"));
                        }
                        Loading => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(t!("Loading..."));
                            });
                        }
                        NoPath => {
                            ui.label(t!("🗙 No path found between the two nodes"));
                        }
                        PathFound(path) => {
                            ui.label(t!("✔ Path found, distance %{dist}", dist = path.len() - 1));

                            let mut del_path = None;
                            let mut cur_path = None;
                            let data = data.read();
                            for (i, id) in path.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui::set_bg_color_tinted(Color32::RED, ui);
                                    self.person_button(&data, ui, id, &mut cur_path);
                                    if i != 0
                                        && i != path.len() - 1
                                        && ui
                                            .button("✖")
                                            .on_hover_text(t!("Exclude this person from the path"))
                                            .clicked()
                                    {
                                        del_path = Some(*id);
                                    }
                                });
                            }
                            if let Some(id) = cur_path {
                                infos.set_infos_current(Some(id));
                            }
                            if let Some(i) = del_path {
                                self.path_dirty = true;
                                self.path_settings.exclude_ids.push(i);
                            }
                        }
                    }
                }
            });
    }
}
