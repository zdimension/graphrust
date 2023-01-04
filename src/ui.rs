use std::collections::{HashSet, VecDeque};
use array_tool::vec::Intersect;
use glium::Display;
use itertools::Itertools;
use crate::{Color3f, combo_with_filter, create_circle_tris, create_rectangle, Vertex, ViewerData};
use derivative::*;
#[derive(Derivative)]
#[derivative(Default)]
pub struct UiState
{
    g_show_nodes: bool,
    #[derivative(Default(value = "true"))]
    g_show_edges: bool,
    infos_current: Option<usize>,
    infos_open: bool,
    path_src: Option<usize>,
    path_dest: Option<usize>,
    found_path: Option<Vec<usize>>,
    exclude_ids: Vec<usize>,
    path_dirty: bool,
    path_no_direct: bool,
    path_no_mutual: bool,
    path_status: String,
    pub path_vbuf: Option<glium::VertexBuffer<Vertex>>,
}


impl UiState
{
    fn set_infos_current(&mut self, id: Option<usize>)
    {
        self.infos_current = id;
        self.infos_open = id.is_some();
    }

    fn do_pathfinding(&mut self, data: &ViewerData, display: &Display)
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

                        self.path_vbuf = Some(glium::VertexBuffer::new(
                            display,
                            &verts).unwrap());

                        return;
                    }
                }
            }
        }
    }

    pub fn draw_ui(&mut self, ui: &mut imgui::Ui, data: &ViewerData, display: &Display)
    {
        ui.window("Graphe")
            .size([400.0, 500.0], imgui::Condition::FirstUseEver)
            .build(||
                {
                    if ui.collapsing_header("Affichage", imgui::TreeNodeFlags::DEFAULT_OPEN)
                    {
                        ui.checkbox("Afficher les nœuds", &mut self.g_show_nodes);
                        ui.checkbox("Afficher les liens", &mut self.g_show_edges);
                    }

                    if ui.collapsing_header("Chemin le plus court", imgui::TreeNodeFlags::DEFAULT_OPEN)
                    {
                        let c1 = combo_with_filter(ui, "#path_src", &mut self.path_src, data);
                        if c1
                        {
                            self.set_infos_current(self.path_src);
                        }
                        ui.same_line();
                        if ui.button("x##src")
                        {
                            self.path_src = None;
                            self.found_path = None;
                        }

                        let c2 = combo_with_filter(ui, "#path_dest", &mut self.path_dest, data);
                        if c2
                        {
                            self.set_infos_current(self.path_dest);
                        }
                        ui.same_line();
                        if ui.button("x##dest")
                        {
                            self.path_dest = None;
                            self.found_path = None;
                        }

                        let exw = ui.calc_item_width();
                        ui.set_next_item_width(exw);
                        ui.text("Exclure :");
                        ui.same_line();
                        if ui.button("x##exclall")
                        {
                            self.exclude_ids.clear();
                        }

                        {
                            let mut cur_excl = None;
                            let mut del_excl = None;
                            for (i, id) in self.exclude_ids.iter().enumerate()
                            {
                                if ui.button_with_size(format!("{}##exclbtn", data.persons[*id].name), [exw, 0.0])
                                {
                                    cur_excl = Some(*id);
                                }
                                ui.same_line();
                                if ui.button(format!("x##delexcl{}", i))
                                {
                                    del_excl = Some(i);
                                }
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

                        if (self.path_dirty || c1 || c2)
                            | ui.checkbox("Éviter chemin direct", &mut self.path_no_direct)
                            | ui.checkbox("Éviter amis communs", &mut self.path_no_mutual)
                        {
                            self.path_dirty = false;
                            self.found_path = None;
                            self.path_vbuf = None;
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

                        ui.text(self.path_status.as_str());

                        let mut del_path = None;
                        let mut cur_path = None;
                        if let Some(ref path) = self.found_path
                        {
                            for (i, id) in path.iter().enumerate()
                            {
                                if ui.button_with_size(format!("{}##pathbtn", data.persons[*id].name), [exw, 0.0])
                                {
                                    cur_path = Some(*id);
                                }
                                if i != 0 && i != path.len() - 1
                                {
                                    ui.same_line();
                                    if ui.button(format!("x##addexcl{}", i))
                                    {
                                        del_path = Some(*id);
                                    }
                                }
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
                    }

                    if ui.collapsing_header("Informations", imgui::TreeNodeFlags::empty())
                    {
                        combo_with_filter(ui, "#infos_user", &mut self.infos_current, data);
                        if let Some(id) = self.infos_current
                        {
                            let person = &data.persons[id];
                            ui.same_line();
                            if ui.button("Ouvrir")
                            {
                                // TODO: crashes on Windows because of a Winit bug
                                /*if let Err(err) = webbrowser::open(format!("https://facebook.com/{}", person.id).as_str()) {
                                    log!("Couldn't open URL: {}", err);
                                };*/
                            }

                            if let Some(_t) = ui.begin_table("#infos", 2)
                            {
                                ui.table_next_row();
                                ui.table_next_column();
                                ui.text("ID Facebook :");
                                ui.table_next_column();
                                ui.text(person.id);
                                ui.table_next_column();
                                ui.text("Amis :");
                                ui.table_next_column();
                                ui.text(format!("{}", person.neighbors.len()));
                                ui.table_next_column();
                                ui.text("Classe :");
                                ui.table_next_column();
                                ui.text(format!("{}", person.modularity_class));
                            }
                        }
                    }
                });
    }

}
