use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use array_tool::vec::Intersect;
use itertools::Itertools;
use derivative::*;
use eframe::{egui_glow, glow};
use egui::{CollapsingHeader, Hyperlink, OpenUrl, Vec2};
use crate::app::ViewerData;
use crate::combo_filter::combo_with_filter;
use crate::geom_draw::{create_circle_tris, create_rectangle};
use crate::graph_storage::Color3f;

#[derive(Derivative)]
#[derivative(Default)]
pub struct UiState
{
    pub g_show_nodes: bool,
    #[derivative(Default(value = "true"))]
    pub g_show_edges: bool,
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
    //pub path_vbuf: Option<VertexBuffer<Vertex>>,
}


impl UiState
{
    fn set_infos_current(&mut self, id: Option<usize>)
    {
        self.infos_current = id;
        self.infos_open = id.is_some();
    }

    fn do_pathfinding(&mut self, data: &ViewerData, _display: ())
    {
        let src_id = self.path_src.unwrap();
        let dest_id = self.path_dest.unwrap();
        let src = &data.persons[src_id];
        let dest = &data.persons[dest_id];

        let intersect = if self.path_no_mutual
        {
            let src_friends = src.neighbors.iter().map(|&(i, _)| i).collect_vec();
            let dest_friends = dest.neighbors.iter().map(|&(i, _)| i).collect_vec();
            HashSet::from_iter(src_friends.intersect(dest_friends))
        } else {
            HashSet::new()
        };

        let exclude_set: HashSet<usize> = HashSet::from_iter(self.exclude_ids.iter().cloned());

        let mut queue = VecDeque::new();
        let mut visited = vec![false; data.persons.len()];
        let mut pred = vec![None; data.persons.len()];
        let mut dist = vec![i32::MAX; data.persons.len()];

        visited[src_id] = true;
        dist[src_id] = 0;
        queue.push_back(src_id);


        while let Some(id) = queue.pop_front()
        {
            let person = &data.persons[id];
            for &(i, _) in person.neighbors.iter()
            {
                if self.path_no_direct && id == src_id && i == dest_id
                {
                    continue;
                }

                if self.path_no_mutual && intersect.contains(&i)
                {
                    continue;
                }

                if exclude_set.contains(&i)
                {
                    continue;
                }

                if !visited[i]
                {
                    visited[i] = true;
                    dist[i] = dist[id] + 1;
                    pred[i] = Some(id);
                    queue.push_back(i);

                    if i == dest_id
                    {
                        let mut path = Vec::new();

                        let mut verts = Vec::new();

                        path.push(dest_id);

                        let mut cur = dest_id;
                        while let Some(p) = pred[cur]
                        {
                            verts.extend(create_rectangle(
                                data.persons[p].position,
                                data.persons[*path.last().unwrap()].position,
                                Color3f::new(1.0, 0.0, 0.0),
                                20.0));
                            path.push(p);
                            cur = p;
                        }

                        verts.extend(
                            path.iter()
                                .flat_map(|&i|
                                    create_circle_tris(data.persons[i].position, 30.0, Color3f::new(0.0, 0.0, 0.0))
                                        .into_iter().chain(
                                        create_circle_tris(data.persons[i].position, 20.0, Color3f::new(1.0, 0.0, 0.0))
                                    )));

                        self.found_path = Some(path);

                        /*self.path_vbuf = Some(VertexBuffer::new(
                            display,
                            &verts).unwrap());*/

                        return;
                    }
                }
            }
        }
    }

    pub fn draw_ui(&mut self, egui: &egui::Context, _frame: &mut eframe::Frame, data: &ViewerData<'_>, display: ())
    {
        egui::SidePanel::left("settings")
            .resizable(false)
            //.max_size([400.0, f32::INFINITY])
            .show(egui, |ui|
                {
                    CollapsingHeader::new("Affichage").default_open(true).show(ui, |ui|
                        {
                            ui.checkbox(&mut self.g_show_nodes, "Afficher les nœuds");
                            ui.checkbox(&mut self.g_show_edges, "Afficher les liens");
                        });

                    CollapsingHeader::new("Chemin le plus court").default_open(true).show(ui, |ui|
                        {
                            let c1 = ui.horizontal(|ui| {
                                let c = combo_with_filter(ui, "#path_src", &mut self.path_src, data);
                                if c.changed()
                                {
                                    self.set_infos_current(self.path_src);
                                }
                                if ui.button("x").clicked()
                                {
                                    self.path_src = None;
                                    self.found_path = None;
                                }
                                c
                            }).inner;

                            let c2 = ui.horizontal(|ui| {
                                let c = combo_with_filter(ui, "#path_dest", &mut self.path_dest, data);
                                if c.changed()
                                {
                                    self.set_infos_current(self.path_dest);
                                }
                                if ui.button("x").clicked()
                                {
                                    self.path_dest = None;
                                    self.found_path = None;
                                }
                                c
                            }).inner;

                            //let exw = ui.calc_item_width();
                            //ui.set_next_item_width(exw);
                            ui.horizontal(|ui| {
                                ui.label("Exclure :");
                                if ui.button("x").clicked()
                                {
                                    self.exclude_ids.clear();
                                }
                            });

                            {
                                let mut cur_excl = None;
                                let mut del_excl = None;
                                for (i, id) in self.exclude_ids.iter().enumerate()
                                {
                                    ui.horizontal(|ui| {
                                        if ui.button(data.persons[*id].name).clicked()
                                        {
                                            cur_excl = Some(*id);
                                        }
                                        if ui.button("x").clicked()
                                        {
                                            del_excl = Some(i);
                                        }
                                    });
                                }
                                if let Some(id) = cur_excl
                                {
                                    self.set_infos_current(Some(id));
                                }
                                if let Some(i) = del_excl
                                {
                                    self.path_dirty = true;
                                    self.exclude_ids.remove(i);
                                }
                            }

                            if (self.path_dirty || c1.changed() || c2.changed())
                                | ui.checkbox(&mut self.path_no_direct, "Éviter chemin direct").changed()
                                | ui.checkbox(&mut self.path_no_mutual, "Éviter amis communs").changed()
                            {
                                self.path_dirty = false;
                                self.found_path = None;
                                //self.path_vbuf = None;
                                self.path_status = match (self.path_src, self.path_dest)
                                {
                                    (Some(x), Some(y)) if x == y => String::from("Source et destination sont identiques"),
                                    (None, _) | (_, None) => String::from(""),
                                    _ =>
                                        {
                                            self.do_pathfinding(data, display);
                                            match self.found_path
                                            {
                                                Some(ref path) => format!("Chemin trouvé, longueur {}", path.len()),
                                                None => String::from("Aucun chemin trouvé"),
                                            }
                                        }
                                }
                            }

                            ui.label(self.path_status.as_str());

                            let mut del_path = None;
                            let mut cur_path = None;
                            if let Some(ref path) = self.found_path
                            {
                                for (i, id) in path.iter().enumerate()
                                {
                                    ui.horizontal(|ui| {
                                        if ui.button(data.persons[*id].name).clicked()
                                        {
                                            cur_path = Some(*id);
                                        }
                                        if i != 0 && i != path.len() - 1
                                        {
                                            if ui.button("x").clicked()
                                            {
                                                del_path = Some(*id);
                                            }
                                        }
                                    });
                                }
                            }
                            if let Some(id) = cur_path
                            {
                                self.set_infos_current(Some(id));
                            }
                            if let Some(i) = del_path
                            {
                                self.path_dirty = true;
                                self.exclude_ids.push(i);
                            }
                        });

                    CollapsingHeader::new("Informations").default_open(true).show(ui, |ui|
                        {
                            combo_with_filter(ui, "#infos_user", &mut self.infos_current, data);
                            if let Some(id) = self.infos_current
                            {
                                let person = &data.persons[id];
                                ui.same_line();
                                if ui.button("Ouvrir").clicked()
                                {
                                    ui.ctx().open_url(OpenUrl {
                                        url: format!("https://facebook.com/{}", person.id),
                                        new_tab: true
                                    });
                                }

                                egui::Grid::new("#infos").show(ui, |ui|
                                    {
                                        ui.label("ID Facebook :");
                                        ui.add(Hyperlink::from_label_and_url(person.id, format!("https://facebook.com/{}", person.id)).open_in_new_tab(true));
                                        ui.end_row();
                                        ui.label("Amis :");
                                        ui.label(format!("{}", person.neighbors.len()));
                                        ui.end_row();
                                        ui.label("Classe :");
                                        ui.label(format!("{}", person.modularity_class));
                                        ui.end_row();
                                    });
                            }
                        });
                });
    }
}

