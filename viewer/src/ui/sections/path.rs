use crate::algorithms::AbstractNode;
use crate::app::ViewerData;
use crate::ui::widgets::combo_filter::{combo_with_filter, COMBO_WIDTH};
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

        let mutual: AHashSet<usize> = if settings.path_no_mutual {
            AHashSet::<_>::from_iter(src.neighbors().iter().copied())
                .intersection(&AHashSet::<_>::from_iter(dest.neighbors().iter().copied()))
                .copied()
                .collect()
        } else {
            AHashSet::new()
        };

        let exclude_set: AHashSet<usize> = AHashSet::from_iter(settings.exclude_ids.iter().cloned());

        let mut queue_f = VecDeque::new();
        let mut queue_b = VecDeque::new();
        let mut visited_f = vec![false; data.len()];
        let mut visited_b = vec![false; data.len()];
        let mut pred_f = vec![None; data.len()];
        let mut pred_b = vec![None; data.len()];

        visited_f[src_id] = true;
        visited_b[dest_id] = true;
        queue_f.push_back(src_id);
        queue_b.push_back(dest_id);

        while let Some(id_f) = queue_f.pop_front() && let Some(id_b) = queue_b.pop_front() {
            let bfs = |current: usize,
                       queue: &mut VecDeque<usize>,
                       visited: &mut Vec<bool>,
                       pred: &mut Vec<Option<usize>>,
                       visited_other: &Vec<bool>| {
                let person = &data[current];
                for &i in person.neighbors().iter() {
                    if settings.path_no_direct && current == src_id && i == dest_id {
                        continue;
                    }

                    if settings.path_no_mutual && mutual.contains(&i) {
                        continue;
                    }

                    if exclude_set.contains(&i) {
                        continue;
                    }

                    if !visited[i] {
                        visited[i] = true;
                        pred[i] = Some(current);
                        if visited_other[i] {
                            return Some(i);
                        }
                        queue.push_back(i);
                    }
                }
                None
            };

            let intersect = bfs(id_f, &mut queue_f, &mut visited_f, &mut pred_f, &visited_b)
                .or_else(|| bfs(id_b, &mut queue_b, &mut visited_b, &mut pred_b, &visited_f));

            if let Some(intersect) = intersect {
                let mut path = vec![intersect];
                let mut cur = intersect;
                while let Some(pred) = pred_f[cur] {
                    path.push(pred);
                    cur = pred;
                }
                path.reverse();
                cur = intersect;
                while let Some(pred) = pred_b[cur] {
                    path.push(pred);
                    cur = pred;
                }
                return Some(PathSectionResults { path });
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

        CollapsingHeader::new(t!("Shortest path"))
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
                    ui.label(t!("Excluded:"));
                    if ui.button("✖").on_hover_text(t!("Clear the exclusion list")).clicked() {
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
                            if ui.button("✖").on_hover_text(t!("Remove from the exclusion list")).clicked() {
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
                    | ui.checkbox(&mut self.path_settings.path_no_direct, t!("Avoid direct link"))
                    .changed()
                    | ui.checkbox(&mut self.path_settings.path_no_mutual, t!("Avoid mutual friends"))
                    .changed()
                {
                    self.path_dirty = false;
                    self.path_status = match (self.path_settings.path_src, self.path_settings.path_dest) {
                        (Some(x), Some(y)) if x == y => Some(PathStatus::SameSrcDest),
                        (None, _) | (_, None) => None,
                        _ => {
                            log::info!("Starting pathfinding");
                            let settings = self.path_settings.clone();
                            let data = data.clone();
                            self.path_thread = Some(thread::spawn(move || {
                                let start = std::time::Instant::now();
                                let data = data.read().persons.clone();
                                let res = Self::do_pathfinding(settings, &data);
                                log::info!("Pathfinding took {:?}", start.elapsed());
                                res
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
                        SameSrcDest => { ui.label(t!("🚫 Source and destination are the same")); }
                        Loading => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(t!("Loading..."));
                            });
                        }
                        NoPath => { ui.label(t!("🗙 No path found between the two nodes")); }
                        PathFound(path) => {
                            ui.label(t!("✔ Path found, distance %{dist}", dist = path.len() - 1));

                            let mut del_path = None;
                            let mut cur_path = None;
                            let data = data.read();
                            for (i, id) in path.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui::set_bg_color_tinted(Color32::RED, ui);
                                    self.person_button(&data, ui, id, &mut cur_path);
                                    if i != 0 && i != path.len() - 1 &&
                                        ui.button("✖").on_hover_text(t!("Exclude this person from the path")).clicked() {
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