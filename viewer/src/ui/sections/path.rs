use crate::algorithms::AbstractNode;
use crate::app::ViewerData;
use crate::combo_filter::{combo_with_filter, COMBO_WIDTH};
use crate::thread;
use crate::thread::JoinHandle;
use crate::threading::MyRwLock;
use crate::ui::infos::InfosSection;
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

#[derive(Derivative)]
#[derivative(Default, Clone)]
pub struct PathSectionSettings {
    pub path_src: Option<usize>,
    pub path_dest: Option<usize>,
    pub exclude_ids: Vec<usize>,
    pub path_no_direct: bool,
    pub path_no_mutual: bool,
}

pub struct PathSectionResults {
    pub path: Vec<usize>,
}

impl PathSection {
    fn do_pathfinding(settings: PathSectionSettings, data: &[impl AbstractNode]) -> Option<PathSectionResults> {
        let src_id = settings.path_src.unwrap();
        let dest_id = settings.path_dest.unwrap();
        let src = &data[src_id];
        let dest = &data[dest_id];

        let intersect: AHashSet<usize> = if settings.path_no_mutual {
            AHashSet::<_>::from_iter(src.neighbors().iter().copied())
                .intersection(&AHashSet::<_>::from_iter(dest.neighbors().iter().copied()))
                .copied()
                .collect()
        } else {
            AHashSet::new()
        };

        let exclude_set: AHashSet<usize> = AHashSet::from_iter(settings.exclude_ids.iter().cloned());

        let mut queue = VecDeque::new();
        let mut visited = vec![false; data.len()];
        let mut pred = vec![None; data.len()];
        let mut dist = vec![i32::MAX; data.len()];

        visited[src_id] = true;
        dist[src_id] = 0;
        queue.push_back(src_id);

        while let Some(id) = queue.pop_front() {
            let person = &data[id];
            for &i in person.neighbors().iter() {
                if settings.path_no_direct && id == src_id && i == dest_id {
                    continue;
                }

                if settings.path_no_mutual && intersect.contains(&i) {
                    continue;
                }

                if exclude_set.contains(&i) {
                    continue;
                }

                if !visited[i] {
                    visited[i] = true;
                    dist[i] = dist[id] + 1;
                    pred[i] = Some(id);
                    queue.push_back(i);

                    if i == dest_id {
                        let mut path = Vec::new();

                        path.push(dest_id);

                        let mut cur = dest_id;
                        while let Some(p) = pred[cur] {
                            path.push(p);
                            cur = p;
                        }

                        return Some(PathSectionResults { path });
                    }
                }
            }
        }
        None
    }

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

        CollapsingHeader::new("Chemin le plus court")
            .default_open(true)
            .show(ui, |ui| {
                let c1 = ui
                    .horizontal(|ui| {
                        ui.radio_value(sel_field, SelectedUserField::PathSource, "");
                        let c = combo_with_filter(ui, "#path_src", &mut self.path_settings.path_src, data);
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
                        let c = combo_with_filter(ui, "#path_dest", &mut self.path_settings.path_dest, data);
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
                    ui.label("Exclure :");
                    if ui.button("✖").on_hover_text("Vider la liste d'exclusion").clicked() {
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
                            if ui.button("✖").on_hover_text("Retirer de la liste d'exclusion").clicked() {
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
                    | ui.checkbox(&mut self.path_settings.path_no_direct, "Éviter chemin direct")
                    .changed()
                    | ui.checkbox(&mut self.path_settings.path_no_mutual, "Éviter amis communs")
                    .changed()
                {
                    self.path_dirty = false;
                    self.path_status = match (self.path_settings.path_src, self.path_settings.path_dest) {
                        (Some(x), Some(y)) if x == y => Some(PathStatus::SameSrcDest),
                        (None, _) | (_, None) => None,
                        _ => {
                            let settings = self.path_settings.clone();
                            let data = data.clone();
                            self.path_thread = Some(thread::spawn(move || {
                                struct SmallNode(Vec<usize>);
                                impl AbstractNode for SmallNode {
                                    fn neighbors(&self) -> &Vec<usize> {
                                        &self.0
                                    }
                                }
                                let data = data.read().persons.iter().map(|p| SmallNode(p.neighbors.clone())).collect_vec();
                                Self::do_pathfinding(settings, &data)
                            }));
                            Some(PathStatus::Loading)
                        }
                    }
                }

                if let Some(st) = &self.path_status {
                    use eframe::epaint::Color32;
                    use PathStatus::*;
                    use crate::ui;
                    match st {
                        SameSrcDest => { ui.label("🚫 Source et destination sont identiques"); }
                        Loading => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Calcul...");
                            });
                        }
                        NoPath => { ui.label("🗙 Aucun chemin entre les deux nœuds"); }
                        PathFound(path) => {
                            ui.label(format!("✔ Chemin trouvé, distance {}", path.len() - 1));

                            let mut del_path = None;
                            let mut cur_path = None;
                            let data = data.read();
                            for (i, id) in path.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui::set_bg_color_tinted(Color32::RED, ui);
                                    self.person_button(&data, ui, id, &mut cur_path);
                                    if i != 0 && i != path.len() - 1 &&
                                        ui.button("✖").on_hover_text("Exclure du chemin").clicked() {
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