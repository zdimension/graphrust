use crate::app::{GraphTab, NewTabRequest, Person, RenderedGraph, Vertex, ViewerData};
use crate::combo_filter::{combo_with_filter, COMBO_WIDTH};
use crate::geom_draw::{create_circle_tris, create_rectangle};
use derivative::*;

use crate::camera::Camera;
use egui::ahash::{AHashMap, AHashSet};
use egui::{
    vec2, CollapsingHeader, Color32, Frame, Hyperlink, Pos2, RichText, Sense, TextStyle, Vec2,
};
use egui_extras::{Column, TableBuilder};
use graph_format::{Color3b, Color3f, EdgeStore, Point};
use itertools::Itertools;
use nalgebra::{DimAdd, Matrix4, Vector2};
use std::collections::{HashSet, VecDeque};
use std::ops::{Range, RangeInclusive};
use std::sync::{Arc, Mutex};

#[derive(Derivative)]
#[derivative(Default)]
pub struct UiState {
    #[derivative(Default(value = "true"))]
    pub g_show_nodes: bool,
    #[derivative(Default(value = "true"))]
    pub g_show_edges: bool,
    pub g_opac_nodes: f32,
    pub g_opac_edges: f32,
    pub infos_current: Option<usize>,
    pub infos_open: bool,
    pub path_src: Option<usize>,
    pub path_dest: Option<usize>,
    pub found_path: Option<Vec<usize>>,
    pub exclude_ids: Vec<usize>,
    pub path_dirty: bool,
    pub path_no_direct: bool,
    pub path_no_mutual: bool,
    pub path_status: String,
    pub path_vbuf: Option<Vec<Vertex>>,
    pub deg_filter_changed: bool,
    pub node_count: usize,
    pub node_count_classes: Vec<(usize, usize)>,
    pub max_degree: u16,
    pub mouse_pos: Option<Pos2>,
    pub mouse_pos_world: Option<Vector2<f32>>,
    pub camera: Matrix4<f32>,
}

impl UiState {
    fn set_infos_current(&mut self, id: Option<usize>) {
        self.infos_current = id;
        self.infos_open = id.is_some();
    }

    fn do_pathfinding(&mut self, data: &ViewerData, graph: &mut RenderedGraph) {
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

    fn refresh_node_count(&mut self, data: &ViewerData<'_>, graph: &mut RenderedGraph) {
        let mut count_classes = vec![0; data.modularity_classes.len()];
        /*self.node_count = data
        .persons
        .iter()
        .filter(|p| {
            let deg = p.neighbors.len() as u16;
            deg >= self.deg_filter.0 && deg <= self.deg_filter.1
        })
        .inspect(|p| todo!())
        .count();*/
        self.node_count = 0;
        for p in &data.persons {
            let deg = p.neighbors.len() as u16;
            if deg >= graph.degree_filter.0 && deg <= graph.degree_filter.1 {
                self.node_count += 1;
                count_classes[p.modularity_class as usize] += 1;
            }
        }
        self.node_count_classes = count_classes
            .iter()
            .enumerate()
            .filter(|(_, &c)| c != 0)
            .sorted_by_key(|(_, &c)| std::cmp::Reverse(c))
            .map(|(i, &c)| (i, c))
            .collect_vec();
    }

    pub fn draw_ui<'a>(
        &mut self,
        ui: &mut egui::Ui,
        data: &ViewerData<'a>,
        graph: &mut RenderedGraph,
        tab_request: &mut Option<NewTabRequest<'a>>,
        frame: &mut eframe::Frame,
    ) {
        egui::SidePanel::left("settings")
            .resizable(false)
            .show_inside(ui, |ui| {
                ui.spacing_mut().slider_width = 200.0;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(10.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.add_space(10.0);
                        let commit = env!("VERGEN_GIT_SHA");
                        ui.label("Commit ");
                        ui.hyperlink_to(
                            commit,
                            format!("https://github.com/zdimension/graphrust/commit/{}", commit),
                        );
                        ui.label(format!(" ({})", env!("VERGEN_BUILD_DATE")));
                    });
                    ui.add_space(10.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.add_space(10.0);
                        ui.label("Si l'interface est ");
                        ui.label(RichText::new("lente").strong());
                        ui.label(":");
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.add_space(10.0);
                        ui.label(" - décocher \"");
                        ui.label(RichText::new("Afficher les liens").underline().strong());
                        ui.label("\"");
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.add_space(10.0);
                        ui.label(" - augmenter \"");
                        ui.label(RichText::new("Degré minimum").underline().strong());
                        ui.label("\"");
                    });
                    ui.add_space(10.0);
                    CollapsingHeader::new("Affichage")
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.checkbox(&mut self.g_show_nodes, "Afficher les nœuds");
                            if self.g_show_nodes {
                                ui.add(
                                    egui::Slider::new(&mut self.g_opac_nodes, 0.0..=1.0)
                                        .text("Opacité")
                                        .clamp_to_range(true),
                                );
                            }
                            ui.checkbox(&mut self.g_show_edges, "Afficher les liens");
                            if self.g_show_edges {
                                ui.add(
                                    egui::Slider::new(&mut self.g_opac_edges, 0.0..=1.0)
                                        .text("Opacité")
                                        .clamp_to_range(true),
                                );
                            }

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
                                self.refresh_node_count(data, graph);
                            }

                            ui.horizontal(|ui| {
                                ui.label("Nœuds affichés :");
                                ui.label(format!("{}", self.node_count));
                            })
                        });

                    CollapsingHeader::new("Chemin le plus court")
                        .default_open(true)
                        .show(ui, |ui| {
                            let c1 = ui
                                .horizontal(|ui| {
                                    let c = combo_with_filter(
                                        ui,
                                        "#path_src",
                                        &mut self.path_src,
                                        data,
                                    );
                                    if c.changed() {
                                        self.set_infos_current(self.path_src);
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
                                    let c = combo_with_filter(
                                        ui,
                                        "#path_dest",
                                        &mut self.path_dest,
                                        data,
                                    );
                                    if c.changed() {
                                        self.set_infos_current(self.path_dest);
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
                                        if ui
                                            .add(
                                                egui::Button::new(data.persons[*id].name)
                                                    .min_size(vec2(COMBO_WIDTH, 0.0)),
                                            )
                                            .clicked()
                                        {
                                            cur_excl = Some(*id);
                                        }
                                        if ui.button("x").clicked() {
                                            del_excl = Some(i);
                                        }
                                    });
                                }
                                if let Some(id) = cur_excl {
                                    self.set_infos_current(Some(id));
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
                                        if ui
                                            .add(
                                                egui::Button::new(data.persons[*id].name)
                                                    .min_size(vec2(COMBO_WIDTH, 0.0)),
                                            )
                                            .clicked()
                                        {
                                            cur_path = Some(*id);
                                        }
                                        if i != 0 && i != path.len() - 1 {
                                            if ui.button("x").clicked() {
                                                del_path = Some(*id);
                                            }
                                        }
                                    });
                                }
                            }
                            if let Some(id) = cur_path {
                                self.set_infos_current(Some(id));
                            }
                            if let Some(i) = del_path {
                                self.path_dirty = true;
                                self.exclude_ids.push(i);
                            }
                        });

                    CollapsingHeader::new("Informations")
                        .default_open(true)
                        .show(ui, |ui| {
                            combo_with_filter(ui, "#infos_user", &mut self.infos_current, data);
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

                                if ui.button("Afficher voisinage").clicked() {
                                    //let mut new_graph = person.neighbors.clone();
                                    //new_graph.push(id);
                                    let new_included = AHashSet::<_>::from_iter(
                                        person.neighbors.iter().copied().chain([id]),
                                    );

                                    let mut new_persons = Vec::with_capacity(256);
                                    //let id_map = vec![None; data.persons.len()];

                                    let mut id_map = AHashMap::new();
                                    let mut class_list = AHashSet::new();

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

                                    let viewer = ViewerData::<'a> {
                                        persons: new_persons,
                                        /*modularity_classes: data
                                        .modularity_classes
                                        .iter()
                                        .enumerate()
                                        .filter(|&(i, _)| class_list.contains(&(i as u16)))
                                        .map(|(_, c)| c.clone())
                                        .collect(),*/
                                        modularity_classes: data.modularity_classes.clone(),
                                        engine: Default::default(),
                                    };

                                    let center =
                                        viewer.persons.iter().map(|p| p.position).sum::<Point>()
                                            / viewer.persons.len() as f32;
                                    let new_tab = GraphTab {
                                        title: format!("Voisinage de {}", person.name),
                                        closeable: true,
                                        camera: Camera::new(center.into()),
                                        cam_animating: None,
                                        ui_state: UiState {
                                            node_count: viewer.persons.len(),
                                            g_opac_edges: 300000.0 / edges.len() as f32,
                                            g_opac_nodes: 40000.0 / viewer.persons.len() as f32,
                                            max_degree: viewer
                                                .persons
                                                .iter()
                                                .map(|p| p.neighbors.len())
                                                .max()
                                                .unwrap()
                                                as u16,
                                            ..UiState::default()
                                        },
                                        rendered_graph: Arc::new(Mutex::new(RenderedGraph::new(
                                            &frame.gl().unwrap().clone(),
                                            &viewer,
                                            edges.iter(),
                                        ))),
                                        viewer_data: viewer,
                                    };
                                    *tab_request = Some(new_tab);
                                }
                            }
                        });

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
                                                let (rect, _) =
                                                    ui.allocate_at_least(size, Sense::hover());
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

                    CollapsingHeader::new("Détails")
                        .default_open(false)
                        .show(ui, |ui| {
                            egui::Grid::new("#mouse_pos").show(ui, |ui| {
                                ui.label("Position :");
                                ui.label(format!("{:?}", self.mouse_pos));
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
                });
            });
    }
}
