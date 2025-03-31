use crate::app::{GraphTabState, Person, ViewerData};
use crate::graph_render::camera::Camera;
use crate::graph_render::GlForwarder;
use crate::threading::{spawn_cancelable, status_pipe, Cancelable, MyRwLock, StatusWriter};
use crate::ui::class::ClassSection;
use crate::ui::modal::ModalWriter;
use crate::ui::path::PathSection;
use crate::ui::tabs::{create_tab, NewTabRequest};
use crate::ui::widgets::combo_filter::{combo_with_filter, COMBO_WIDTH};
use crate::ui::{ParadoxState, SelectedUserField, UiState};
use crate::{for_progress, log, ui};
use ahash::{AHashMap, AHashSet};
use derivative::Derivative;
use eframe::emath::vec2;
use eframe::epaint::Color32;
use egui::{CollapsingHeader, Hyperlink, Id, SliderClamping, Ui};
use graph_format::EdgeStore;
use itertools::Itertools;
use std::sync::{mpsc, Arc};

#[derive(Derivative)]
#[derivative(Default)]
pub struct InfosSection {
    pub infos_current: Option<usize>,
    pub infos_open: bool,
    #[derivative(Default(value = "1"))]
    pub neighborhood_degree: usize,
    pub paradox: ParadoxState,
}

impl InfosSection {
    pub(crate) fn set_infos_current(&mut self, id: Option<usize>) {
        self.infos_current = id;
        self.infos_open = id.is_some();
    }

    pub(crate) fn show(
        &mut self,
        data_rw: &Arc<MyRwLock<ViewerData>>,
        tab_request: &mut Option<NewTabRequest>,
        ui: &mut Ui,
        camera: &Camera,
        path_section: &PathSection,
        sel_field: &mut SelectedUserField,
        modal: &impl ModalWriter,
    ) {
        CollapsingHeader::new(t!("Infos"))
            .id_salt("infos")
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui::set_bg_color_tinted(Color32::GREEN, ui);
                    ui.radio_value(sel_field, SelectedUserField::Selected, "");
                    combo_with_filter(ui, "#infos_user", &mut self.infos_current, data_rw);
                });
                if let Some(id) = self.infos_current {
                    let data = &*data_rw.read();
                    let person = &data.persons[id];
                    let class = person.modularity_class;

                    egui::Grid::new("#infos").show(ui, |ui| {
                        ui.label(t!("Facebook ID:"));
                        ui.horizontal(|ui| {
                            ui.add(
                                Hyperlink::from_label_and_url(
                                    person.id,
                                    format!("https://facebook.com/{}", person.id),
                                )
                                    .open_in_new_tab(true),
                            );
                            let copy = ui.button("🗐");
                            if copy.clicked() {
                                let text = if ui.input(|is| is.modifiers.shift) {
                                    format!("bfs(\"{}\", 1)", person.id)
                                } else {
                                    person.id.to_string()
                                };
                                ui.output_mut(|out| out.copied_text = text);
                            }
                        });
                        ui.end_row();
                        ui.label(t!("Friends:"));
                        ui.label(format!("{}", person.neighbors.len()));
                        ui.end_row();
                        ui.label(t!("Class:"));
                        ui.horizontal(|ui| {
                            ClassSection::class_circle(ui, &data.modularity_classes[class as usize]);
                            self.create_class_subgraph(data_rw, tab_request, camera, path_section, modal, class, ui);
                        });
                        ui.end_row();
                    });

                    CollapsingHeader::new(t!("Friends"))
                        .id_salt("friends")
                        .default_open(false)
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical().max_height(200.0).show(
                                ui,
                                |ui| {
                                    for (neighb, name) in person
                                        .neighbors
                                        .iter()
                                        .map(|&i| (i, data.persons[i].name))
                                        .sorted_unstable_by(|(_, a), (_, b)| a.cmp(b))
                                    {
                                        if ui
                                            .add(egui::Button::new(name).min_size(
                                                vec2(COMBO_WIDTH - 18.0, 0.0),
                                            ))
                                            .clicked()
                                        {
                                            self.set_infos_current(Some(neighb));
                                        }
                                    }
                                },
                            );
                        });

                    CollapsingHeader::new(t!("Friendship paradox"))
                        .id_salt("paradox")
                        .default_open(false)
                        .show(ui, |ui| {
                            if self.paradox.current != self.infos_current {
                                let mut sum = 0;
                                let friends = person.neighbors.iter()
                                    .map(|&i| data.persons[i].neighbors.len())
                                    .inspect(|n| sum += n)
                                    .minmax();
                                use itertools::MinMaxResult::*;
                                let (min, max) = match friends {
                                    NoElements => (0, 0),
                                    OneElement(n) => (n, n),
                                    MinMax(min, max) => (min, max),
                                };
                                self.paradox = ParadoxState { current: Some(id), sum, min, max };
                            }

                            let state = &self.paradox;

                            egui::Grid::new("#paradox").show(ui, |ui| {
                                ui.label(t!("Friends:"));
                                ui.label(format!("{}", person.neighbors.len()));
                                ui.end_row();
                                ui.label(t!("Friends of friends (average):"));
                                ui.label(format!("{}", state.sum / person.neighbors.len()));
                                ui.end_row();
                                ui.label(t!("Friends of friends (min):"));
                                ui.label(format!("{}", state.min));
                                ui.end_row();
                                ui.label(t!("Friends of friends (max):"));
                                ui.label(format!("{}", state.max));
                                ui.end_row();
                            });
                        });

                    ui.horizontal(|ui| {
                        ui.style_mut().spacing.slider_width = 100.0;
                        ui.add(
                            egui::Slider::new(&mut self.neighborhood_degree, 1..=13)
                                .text(t!("Degree"))
                                .clamping(SliderClamping::Always),
                        );

                        if ui.button(t!("Show neighborhood"))
                            .on_hover_text(t!("Show friends up to a certain distance from the person. Degree 1 will show direct friends, degree 2 friends of friends, etc."))
                            .clicked() {
                            let neighborhood_degree = self.neighborhood_degree;
                            self.create_subgraph(
                                t!("%{deg}-neighborhood of %{name}", deg = neighborhood_degree, name = person.name).to_string(),
                                data_rw, tab_request, camera, path_section, ui, modal.clone(),
                                move |status_tx, data| {
                                    let mut new_included = AHashSet::from([id]);
                                    let mut last_batch = AHashSet::from([id]);
                                    for i in 0..neighborhood_degree {
                                        let mut new_friends = AHashSet::new();
                                        for person in last_batch.iter() {
                                            new_friends.extend(
                                                data.persons[*person]
                                                    .neighbors
                                                    .iter()
                                                    .copied()
                                                    .filter(|&i| !new_included.contains(&i)),
                                            );
                                        }
                                        if new_friends.is_empty() {
                                            log!(status_tx, t!("No new friends at degree %{deg}", deg = i + 1));
                                            if last_batch.len() < 50 {
                                                log!(status_tx, "{}: {:?}", t!("At %{deg}", deg = i), last_batch.iter().map(|i| data.persons[*i].name).collect::<Vec<_>>());
                                            }
                                            break;
                                        }
                                        new_included.extend(new_friends.iter().copied());
                                        log!(status_tx, t!("%{num} new friends at degree %{deg}", num = new_friends.len(), deg = i + 1));
                                        last_batch = new_friends;
                                    }

                                    log!(status_tx, t!("Got %{len} friends", len = new_included.len()));
                                    Ok(new_included)
                                });
                        }
                    });
                }
            });
    }

    pub(crate) fn create_class_subgraph(
        &self,
        data_rw: &Arc<MyRwLock<ViewerData>>,
        tab_request: &mut Option<NewTabRequest>,
        camera: &Camera,
        path_section: &PathSection,
        modal: &impl ModalWriter,
        class: u16,
        ui: &mut Ui,
    ) {
        if ui.button(format!("{}", class)).clicked() {
            self.create_subgraph(
                t!("Class %{class}", class = class).to_string(),
                data_rw,
                tab_request,
                camera,
                path_section,
                ui,
                modal.clone(),
                move |_, data| {
                    Ok(data
                        .persons
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| p.modularity_class == class)
                        .map(|(i, _)| i)
                        .collect())
                },
            );
        }
    }

    fn create_subgraph(
        &self,
        title: String,
        data: &Arc<MyRwLock<ViewerData>>,
        tab_request: &mut Option<NewTabRequest>,
        camera: &Camera,
        path_section: &PathSection,
        ui: &mut Ui,
        modal_tx: impl ModalWriter,
        x: impl FnOnce(&StatusWriter, &ViewerData) -> Cancelable<AHashSet<usize>> + Send + 'static,
    ) {
        let (status_tx, status_rx) = status_pipe(ui.ctx());
        let (state_tx, state_rx) = mpsc::channel();
        let (gl_fwd, gl_mpsc) = GlForwarder::new();

        *tab_request = Some(NewTabRequest {
            id: Id::new((&title, chrono::Utc::now())),
            title,
            closeable: true,
            state: GraphTabState::loading(status_rx, state_rx, gl_mpsc),
        });

        let infos_current = self.infos_current;
        let path_src = path_section.path_settings.path_src;
        let path_dest = path_section.path_settings.path_dest;
        let camera = *camera;

        let data = data.clone();
        spawn_cancelable(modal_tx, move || {
            let new_included = x(&status_tx, &data.read())?;

            let mut new_persons = Vec::with_capacity(new_included.len());
            let mut new_neighbors = Vec::with_capacity(new_included.len());

            let mut id_map = AHashMap::new();
            let mut class_list = AHashSet::new();

            log!(status_tx, t!("Processing person list and creating ID map"));
            {
                let data = data.read();
                for &id in new_included.iter() {
                    let pers = &data.persons[id];
                    id_map.insert(id, new_persons.len());
                    class_list.insert(pers.modularity_class);
                    new_persons.push(Person { ..*pers });
                    new_neighbors.push(vec![]);
                }
            }

            let mut edges = Vec::new();

            log!(status_tx, t!("Creating new neighbor lists and edge list"));
            {
                let data = data.read();
                for_progress!(status_tx, (&old_id, &new_id) in id_map.iter(), {
                    new_neighbors[new_id].extend(
                        data.persons[old_id]
                            .neighbors
                            .iter()
                            .filter_map(|&i| id_map.get(&i)),
                    );
                    for &nb in new_neighbors[new_id].iter() {
                        if new_id < nb {
                            edges.push(EdgeStore {
                                a: new_id as u32,
                                b: nb as u32,
                            });
                        } else {
                            // we do nothing since we'll get it eventually
                        }
                    }
                });
                for_progress!(status_tx, (person, nblist) in new_persons.iter_mut().zip(new_neighbors.iter()), {
                    // SAFETY: neighbor_lists is kept alive
                    person.neighbors = unsafe { std::mem::transmute(nblist.as_slice()) };
                });
            }

            log!(status_tx, t!("Computing min edge filter"));

            let mut filter = 1;
            const MAX: usize = 10000;
            while new_persons
                .iter()
                .filter(|p| p.neighbors.len() as u16 >= filter)
                .enumerate()
                .skip(MAX)
                .next()
                .is_some()
            {
                // count() would iterate all the nodes
                filter += 1;
            }

            let viewer = ViewerData::new(
                new_persons,
                new_neighbors,
                data.read().modularity_classes.clone(),
            )?;

            let mut new_ui = UiState::default();

            // match path and selection
            macro_rules! match_id {
                ($field:expr, $self_expr:expr) => {
                    if let Some(current) = $self_expr {
                        if let Some(new_id) = id_map.get(&current) {
                            $field = Some(*new_id);
                        }
                    }
                };
            }
            match_id!(new_ui.infos.infos_current, infos_current);
            match_id!(new_ui.path.path_settings.path_src, path_src);
            match_id!(new_ui.path.path_settings.path_dest, path_dest);
            new_ui.path.path_dirty = true;

            state_tx.send(create_tab(
                viewer,
                edges.iter(),
                gl_fwd,
                filter,
                camera,
                new_ui,
                status_tx,
            )?)?;

            Ok(())
        });
    }
}
