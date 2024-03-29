use crate::app::{create_tab, NewTabRequest, Person, RenderedGraph, Vertex, ViewerData};
use crate::combo_filter::{combo_with_filter, COMBO_WIDTH};
use crate::geom_draw::{create_circle_tris, create_rectangle};
use derivative::*;

use crate::camera::Camera;
use eframe::Frame;
use egui::ahash::{AHashMap, AHashSet};
use egui::{vec2, CollapsingHeader, Color32, Hyperlink, Pos2, Sense, Ui, Vec2, Visuals};
use egui_extras::{Column, TableBuilder};
use graph_format::{Color3b, Color3f, EdgeStore};
use itertools::Itertools;
use nalgebra::{Matrix4, Vector2};
use std::collections::VecDeque;
use std::ops::RangeInclusive;

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

#[derive(Derivative)]
#[derivative(Default)]
pub struct PathSection {
    pub path_src: Option<usize>,
    pub path_dest: Option<usize>,
    pub found_path: Option<Vec<usize>>,
    pub exclude_ids: Vec<usize>,
    pub path_dirty: bool,
    pub path_no_direct: bool,
    pub path_no_mutual: bool,
    pub path_status: String,
    pub path_vbuf: Option<Vec<Vertex>>,
}

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

impl PathSection {
    fn do_pathfinding(&mut self, data: &ViewerData<'_>, graph: &mut RenderedGraph) {
        let src_id = self.path_src.unwrap();
        let dest_id = self.path_dest.unwrap();
        let src = &data.persons[src_id];
        let dest = &data.persons[dest_id];

        let intersect: AHashSet<usize> = if self.path_no_mutual {
            AHashSet::<_>::from_iter(src.neighbors.iter().copied())
                .intersection(&AHashSet::<_>::from_iter(dest.neighbors.iter().copied()))
                .copied()
                .collect()
        } else {
            AHashSet::new()
        };

        let exclude_set: AHashSet<usize> = AHashSet::from_iter(self.exclude_ids.iter().cloned());

        let mut queue = VecDeque::new();
        let mut visited = vec![false; data.persons.len()];
        let mut pred = vec![None; data.persons.len()];
        let mut dist = vec![i32::MAX; data.persons.len()];

        visited[src_id] = true;
        dist[src_id] = 0;
        queue.push_back(src_id);

        while let Some(id) = queue.pop_front() {
            let person = &data.persons[id];
            for &i in person.neighbors.iter() {
                if self.path_no_direct && id == src_id && i == dest_id {
                    continue;
                }

                if self.path_no_mutual && intersect.contains(&i) {
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

                        let mut verts = Vec::new();

                        path.push(dest_id);

                        let mut cur = dest_id;
                        while let Some(p) = pred[cur] {
                            verts.extend(create_rectangle(
                                data.persons[p].position,
                                data.persons[*path.last().unwrap()].position,
                                Color3f::new(1.0, 0.0, 0.0),
                                Color3f::new(1.0, 0.0, 0.0),
                                20.0,
                            ));
                            path.push(p);
                            cur = p;
                        }

                        verts.extend(path.iter().flat_map(|&i| {
                            create_circle_tris(
                                data.persons[i].position,
                                30.0,
                                Color3f::new(0.0, 0.0, 0.0),
                            )
                            .into_iter()
                            .chain(create_circle_tris(
                                data.persons[i].position,
                                20.0,
                                Color3f::new(1.0, 0.0, 0.0),
                            ))
                        }));

                        self.found_path = Some(path);

                        graph.new_path = Some(verts);

                        return;
                    }
                }
            }
        }
    }

    fn person_button(
        &self,
        data: &ViewerData<'_>,
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

    fn show(
        &mut self,
        data: &ViewerData<'_>,
        graph: &mut RenderedGraph,
        ui: &mut Ui,
        infos: &mut InfosSection,
        sel_field: &mut SelectedUserField,
    ) {
        CollapsingHeader::new("Chemin le plus court")
            .default_open(true)
            .show(ui, |ui| {
                let c1 = ui
                    .horizontal(|ui| {
                        ui.radio_value(sel_field, SelectedUserField::PathSource, "");
                        let c = combo_with_filter(ui, "#path_src", &mut self.path_src, data);
                        if c.changed() {
                            infos.set_infos_current(self.path_src);
                        }
                        if ui.button("x").clicked() {
                            self.path_src = None;
                            self.found_path = None;
                            graph.new_path = Some(vec![]);
                        }
                        c
                    })
                    .inner;

                let c2 = ui
                    .horizontal(|ui| {
                        ui.radio_value(sel_field, SelectedUserField::PathDest, "");
                        let c = combo_with_filter(ui, "#path_dest", &mut self.path_dest, data);
                        if c.changed() {
                            infos.set_infos_current(self.path_dest);
                        }
                        if ui.button("x").clicked() {
                            self.path_dest = None;
                            self.found_path = None;
                            graph.new_path = Some(vec![]);
                        }
                        c
                    })
                    .inner;

                ui.horizontal(|ui| {
                    ui.label("Exclure :");
                    if ui.button("x").clicked() {
                        self.exclude_ids.clear();
                        self.path_dirty = true;
                    }
                });

                {
                    let mut cur_excl = None;
                    let mut del_excl = None;
                    for (i, id) in self.exclude_ids.iter().enumerate() {
                        ui.horizontal(|ui| {
                            self.person_button(data, ui, id, &mut cur_excl);
                            if ui.button("x").clicked() {
                                del_excl = Some(i);
                            }
                        });
                    }
                    if let Some(id) = cur_excl {
                        infos.set_infos_current(Some(id));
                    }
                    if let Some(i) = del_excl {
                        self.path_dirty = true;
                        self.exclude_ids.remove(i);
                    }
                }

                if (self.path_dirty || c1.changed() || c2.changed())
                    | ui.checkbox(&mut self.path_no_direct, "Éviter chemin direct")
                        .changed()
                    | ui.checkbox(&mut self.path_no_mutual, "Éviter amis communs")
                        .changed()
                {
                    self.path_dirty = false;
                    self.found_path = None;
                    graph.new_path = Some(vec![]);
                    self.path_status = match (self.path_src, self.path_dest) {
                        (Some(x), Some(y)) if x == y => {
                            String::from("Source et destination sont identiques")
                        }
                        (None, _) | (_, None) => String::from(""),
                        _ => {
                            self.do_pathfinding(data, graph);
                            match self.found_path {
                                Some(ref path) => {
                                    format!("Chemin trouvé, longueur {}", path.len())
                                }
                                None => String::from("Aucun chemin trouvé"),
                            }
                        }
                    }
                }

                if !self.path_status.is_empty() {
                    ui.label(self.path_status.as_str());
                }

                let mut del_path = None;
                let mut cur_path = None;
                if let Some(ref path) = self.found_path {
                    for (i, id) in path.iter().enumerate() {
                        ui.horizontal(|ui| {
                            set_bg_color_tinted(Color32::RED, ui);
                            self.person_button(data, ui, id, &mut cur_path);
                            if i != 0 && i != path.len() - 1 {
                                if ui.button("x").clicked() {
                                    del_path = Some(*id);
                                }
                            }
                        });
                    }
                }
                if let Some(id) = cur_path {
                    infos.set_infos_current(Some(id));
                }
                if let Some(i) = del_path {
                    self.path_dirty = true;
                    self.exclude_ids.push(i);
                }
            });
    }
}

#[derive(Derivative)]
#[derivative(Default)]
pub struct ClassSection {
    pub node_count_classes: Vec<(usize, usize)>,
}

#[derive(Derivative)]
#[derivative(Default)]
pub struct InfosSection {
    pub infos_current: Option<usize>,
    pub infos_open: bool,
    #[derivative(Default(value = "1"))]
    pub neighborhood_degree: usize,
}

impl InfosSection {
    fn set_infos_current(&mut self, id: Option<usize>) {
        self.infos_current = id;
        self.infos_open = id.is_some();
    }

    fn show<'data>(
        &mut self,
        data: &ViewerData<'data>,
        tab_request: &mut Option<NewTabRequest<'data>>,
        frame: &mut Frame,
        ui: &mut Ui,
        camera: &Camera,
        path_section: &PathSection,
        sel_field: &mut SelectedUserField,
    ) {
        CollapsingHeader::new("Informations")
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    set_bg_color_tinted(Color32::GREEN, ui);
                    ui.radio_value(sel_field, SelectedUserField::Selected, "");
                    combo_with_filter(ui, "#infos_user", &mut self.infos_current, data);
                });
                if let Some(id) = self.infos_current {
                    let person = &data.persons[id];

                    egui::Grid::new("#infos").show(ui, |ui| {
                        ui.label("ID Facebook :");
                        ui.add(
                            Hyperlink::from_label_and_url(
                                person.id,
                                format!("https://facebook.com/{}", person.id),
                            )
                                .open_in_new_tab(true),
                        );
                        ui.end_row();
                        ui.label("Amis :");
                        ui.label(format!("{}", person.neighbors.len()));
                        ui.end_row();
                        ui.label("Classe :");
                        ui.label(format!("{}", person.modularity_class));
                        ui.end_row();
                    });

                    CollapsingHeader::new("Liste d'amis")
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

                    ui.horizontal(|ui| {
                        ui.style_mut().spacing.slider_width = 100.0;
                        ui.add(
                            egui::Slider::new(&mut self.neighborhood_degree, 1..=10)
                                .text("Degré")
                                .clamp_to_range(true),
                        );

                        if ui.button("Afficher voisinage")
                            .on_hover_text("Afficher les amis jusqu'à une certaine distance de la personne. Le degré 1 affichera les amis directs, le degré 2 les amis des amis, etc.")
                            .clicked() {
                            let mut new_included = AHashSet::from([id]);
                            let mut last_batch = AHashSet::from([id]);
                            for i in 0..self.neighborhood_degree {
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
                                    log::info!("No new friends at degree {}", i + 1);
                                    if last_batch.len() < 50 {
                                        log::info!("At {}: {:?}", i, last_batch.iter().map(|i| data.persons[*i].name).collect::<Vec<_>>());
                                    }
                                    break;
                                }
                                new_included.extend(new_friends.iter().copied());
                                log::info!("{} new friends at degree {}", new_friends.len(), i + 1);
                                last_batch = new_friends;
                            }

                            log::info!("Got {} persons total", new_included.len());

                            let mut new_persons =
                                Vec::with_capacity(new_included.len());

                            let mut id_map = AHashMap::new();
                            let mut class_list = AHashSet::new();

                            log::info!("Processing person list and creating ID map");
                            for &id in new_included.iter() {
                                let pers = &data.persons[id];
                                id_map.insert(id, new_persons.len());
                                class_list.insert(pers.modularity_class);
                                new_persons.push(Person {
                                    neighbors: vec![],
                                    ..*pers
                                });
                            }

                            let mut edges = AHashSet::new();

                            log::info!("Creating new neighbor lists and edge list");
                            for (&old_id, &new_id) in id_map.iter() {
                                new_persons[new_id].neighbors.extend(
                                    data.persons[old_id]
                                        .neighbors
                                        .iter()
                                        .filter_map(|&i| id_map.get(&i)),
                                );
                                for &nb in new_persons[new_id].neighbors.iter() {
                                    let [a, b] = std::cmp::minmax(new_id, nb);
                                    edges.insert(EdgeStore {
                                        a: a as u32,
                                        b: b as u32,
                                    });
                                }
                            }

                            let mut filter = 1;
                            while new_persons.iter().filter(|p| p.neighbors.len() as u16 >= filter).count() > 10000 {
                                filter += 1;
                            }

                            let viewer = ViewerData::new(new_persons, data.modularity_classes.clone());

                            let mut new_ui = UiState::default();

                            // match path and selection
                            macro_rules! match_id {
                                ($field:expr, $self_expr:expr) => {
                                    if let Some(current) = $self_expr {
                                        if let Some(new_id) = id_map.get(&current) {
                                            $field = Some(*new_id);
                                        }
                                    }
                                }
                            }
                            match_id!(new_ui.infos.infos_current, self.infos_current);
                            match_id!(new_ui.path.path_src, path_section.path_src);
                            match_id!(new_ui.path.path_dest, path_section.path_dest);
                            new_ui.path.path_dirty = true;

                            *tab_request = Some(create_tab(
                                format!("{}-voisinage de {}", self.neighborhood_degree, person.name),
                                viewer,
                                edges.iter(),
                                &frame.gl().unwrap().clone(),
                                filter,
                                camera.clone(),
                                new_ui
                            ));
                        }
                    });
                }
            });
    }
}

#[derive(Derivative)]
#[derivative(Default)]
pub struct DetailsSection {
    pub mouse_pos: Option<Pos2>,
    pub mouse_pos_world: Option<Vector2<f32>>,
    pub camera: Matrix4<f32>,
}

impl DetailsSection {
    fn show(&mut self, ui: &mut Ui) {
        CollapsingHeader::new("Détails")
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("#mouse_pos").show(ui, |ui| {
                    ui.label("Position :");
                    ui.label(format!(
                        "{:?}",
                        self.mouse_pos.map(|p| Vector2::new(p.x, p.y))
                    ));
                    ui.end_row();
                    ui.label("Position (monde) :");
                    ui.label(format!("{:?}", self.mouse_pos_world));
                    ui.end_row();
                });
                egui::Grid::new("#cammatrix").show(ui, |ui| {
                    for i in 0..4 {
                        for j in 0..4 {
                            // format fixed width
                            ui.label(format!("{:.3}", self.camera[(i, j)]));
                        }
                        ui.end_row();
                    }
                });
            });
    }
}

#[derive(Default, PartialEq, Eq)]
pub enum SelectedUserField {
    Selected,
    #[default]
    PathSource,
    PathDest,
}

#[derive(Default)]
pub struct UiState {
    pub display: DisplaySection,
    pub path: PathSection,
    pub classes: ClassSection,
    pub infos: InfosSection,
    pub details: DetailsSection,
    pub selected_user_field: SelectedUserField,
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

impl ClassSection {
    fn show(&mut self, data: &ViewerData<'_>, ui: &mut Ui) {
        CollapsingHeader::new("Classes")
            .default_open(false)
            .show(ui, |ui| {
                TableBuilder::new(ui)
                    .column(Column::exact(20.0))
                    .column(Column::exact(40.0))
                    .column(Column::exact(40.0))
                    .body(|mut body| {
                        for &(clid, count) in &self.node_count_classes {
                            body.row(15.0, |mut row| {
                                let cl = &data.modularity_classes[clid];
                                let rad = 5.0;
                                let size = Vec2::splat(2.0 * rad + 5.0);
                                row.col(|ui| {
                                    let (rect, _) = ui.allocate_at_least(size, Sense::hover());
                                    let Color3b { r, g, b } = cl.color.to_u8();
                                    ui.painter().circle_filled(
                                        rect.center(),
                                        rad,
                                        Color32::from_rgb(r / 2, g / 2, b / 2),
                                    );
                                });
                                row.col(|ui| {
                                    ui.label(format!("{}", cl.id));
                                });
                                row.col(|ui| {
                                    ui.label(format!("{}", count));
                                });
                            });
                        }
                    });
            });
    }
}

impl DisplaySection {
    fn show(&mut self, graph: &mut RenderedGraph, ui: &mut Ui) {
        CollapsingHeader::new("Affichage")
            .default_open(true)
            .show(ui, |ui| {
                ui.checkbox(&mut self.g_show_nodes, "Afficher les nœuds");
                if self.g_show_nodes {
                    ui.add(
                        egui::Slider::new(&mut self.g_opac_nodes, 0.0..=1.0)
                            .text("Opacité")
                            .custom_formatter(percent_formatter)
                            .custom_parser(percent_parser)
                            .clamp_to_range(true),
                    );
                }
                ui.checkbox(&mut self.g_show_edges, "Afficher les liens");
                if self.g_show_edges {
                    ui.add(
                        egui::Slider::new(&mut self.g_opac_edges, 0.0..=1.0)
                            .text("Opacité")
                            .custom_formatter(percent_formatter)
                            .custom_parser(percent_parser)
                            .clamp_to_range(true),
                    );
                }

                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        let start = ui
                            .add(
                                egui::DragValue::new(&mut graph.degree_filter.0)
                                    .speed(1)
                                    .clamp_range(1..=graph.degree_filter.1)
                                    .prefix("Degré minimum : "),
                            )
                            .changed();
                        let end = ui
                            .add(
                                egui::DragValue::new(&mut graph.degree_filter.1)
                                    .speed(1)
                                    .clamp_range(graph.degree_filter.0..=self.max_degree)
                                    .prefix("Degré maximum : "),
                            )
                            .changed();
                        if start || end {
                            self.deg_filter_changed = true;
                        }
                    });
                    ui.vertical(|ui| {
                        ui.checkbox(&mut graph.filter_nodes, "Filtrer les nœuds");
                    });
                });

                ui.horizontal(|ui| {
                    ui.label("Nœuds affichés :");
                    ui.label(format!("{}", self.node_count));
                });
            });
    }
}

impl UiState {
    fn refresh_node_count(&mut self, data: &ViewerData<'_>, graph: &mut RenderedGraph) {
        let mut count_classes = vec![0; data.modularity_classes.len()];
        self.display.node_count = 0;
        for p in &data.persons {
            let ok = if graph.filter_nodes {
                let deg = p.neighbors.len() as u16;
                deg >= graph.degree_filter.0 && deg <= graph.degree_filter.1
            } else {
                true
            };
            if ok {
                self.display.node_count += 1;
                count_classes[p.modularity_class as usize] += 1;
            }
        }
        self.classes.node_count_classes = count_classes
            .iter()
            .enumerate()
            .filter(|(_, &c)| c != 0)
            .sorted_by_key(|(_, &c)| std::cmp::Reverse(c))
            .map(|(i, &c)| (i, c))
            .collect_vec();
    }

    pub fn draw_ui<'a>(
        &mut self,
        ui: &mut Ui,
        data: &ViewerData<'a>,
        graph: &mut RenderedGraph,
        tab_request: &mut Option<NewTabRequest<'a>>,
        frame: &mut Frame,
        camera: &Camera,
    ) {
        ui.spacing_mut().slider_width = 200.0;
        egui::ScrollArea::vertical().show(ui, |ui| {
            self.display.show(graph, ui);
            if self.display.deg_filter_changed {
                self.refresh_node_count(data, graph);
                self.display.deg_filter_changed = false;
            }

            self.path.show(
                data,
                graph,
                ui,
                &mut self.infos,
                &mut self.selected_user_field,
            );

            self.infos.show(
                &data,
                tab_request,
                frame,
                ui,
                camera,
                &self.path,
                &mut self.selected_user_field,
            );

            self.classes.show(&data, ui);

            self.details.show(ui);
        });
    }
}
