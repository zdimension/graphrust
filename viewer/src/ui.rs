use crate::app::{create_tab, show_progress_bar, spawn_cancelable, status_pipe, CamAnimating, Cancelable, ContextUpdater, GlForwarder, GlTask, GraphTabState, ModalWriter, ModularityClass, MyRwLock, NewTabRequest, NodeFilter, Person, PersonVertex, RenderedGraph, StatusReader, StatusWriter, TabCamera, Vertex, ViewerData};
use crate::combo_filter::{combo_with_filter, COMBO_WIDTH};
use crate::{for_progress, log, log_progress, try_log_progress};
use derivative::*;

use crate::app::thread;
use crate::camera::Camera;
use ahash::{AHashMap, AHashSet};
use eframe::glow::HasContext;
use eframe::{glow, Frame};
use egui::{vec2, CollapsingHeader, Color32, Hyperlink, Id, Pos2, Sense, SliderClamping, Ui, Vec2};
use egui_extras::{Column, TableBuilder};
use forceatlas2::{Layout, Node, Settings, VecN};
use graph_format::nalgebra::{Matrix4, Vector2};
use graph_format::{Color3b, EdgeStore, Point};
use itertools::{Itertools, MinMaxResult};
use parking_lot::{Mutex, RwLock};
use std::collections::VecDeque;
use std::ops::RangeInclusive;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, RecvError, Sender, TryRecvError};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};
use thread::JoinHandle;

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
    pub path_settings: PathSectionSettings,
    pub path_dirty: bool,
    pub path_status: Option<PathStatus>,
    pub path_channel: Option<Receiver<Option<PathSectionResults>>>,
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
    fn do_pathfinding(settings: PathSectionSettings, data: &ViewerData, tx: Sender<Option<PathSectionResults>>, ctx: ContextUpdater) {
        let src_id = settings.path_src.unwrap();
        let dest_id = settings.path_dest.unwrap();
        let src = &data.persons[src_id];
        let dest = &data.persons[dest_id];

        let intersect: AHashSet<usize> = if settings.path_no_mutual {
            AHashSet::<_>::from_iter(src.neighbors.iter().copied())
                .intersection(&AHashSet::<_>::from_iter(dest.neighbors.iter().copied()))
                .copied()
                .collect()
        } else {
            AHashSet::new()
        };

        let exclude_set: AHashSet<usize> = AHashSet::from_iter(settings.exclude_ids.iter().cloned());

        let mut queue = VecDeque::new();
        let mut visited = vec![false; data.persons.len()];
        let mut pred = vec![None; data.persons.len()];
        let mut dist = vec![i32::MAX; data.persons.len()];

        visited[src_id] = true;
        dist[src_id] = 0;
        queue.push_back(src_id);

        let result = 'path_result: {
            while let Some(id) = queue.pop_front() {
                let person = &data.persons[id];
                for &i in person.neighbors.iter() {
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

                            /*self.found_path = Some(path);

                            graph.new_path = Some(verts);*/

                            /*let _ = tx.send(Some(PathSectionResults { path, verts }));

                            return;*/

                            break 'path_result Some(PathSectionResults { path });
                        }
                    }
                }
            }
            None
        };

        if tx.send(result).is_err() {
            // tab closed
        }

        ctx.update();
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

    fn show(
        &mut self,
        data: &Arc<MyRwLock<ViewerData>>,
        ui: &mut Ui,
        infos: &mut InfosSection,
        sel_field: &mut SelectedUserField,
    ) {
        if let Some(rx) = self.path_channel.as_ref() {
            if let Ok(res) = rx.try_recv() {
                if let Some(res) = res {
                    self.path_status = Some(PathStatus::PathFound(res.path));
                } else {
                    self.path_status = Some(PathStatus::NoPath);
                }
                self.path_channel = None;
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
                        if ui.button("x").clicked() {
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
                        if ui.button("x").clicked() {
                            self.path_settings.path_dest = None;
                        }
                        c
                    })
                    .inner;

                ui.horizontal(|ui| {
                    ui.label("Exclure :");
                    if ui.button("x").clicked() {
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
                            let (tx, rx) = mpsc::channel();
                            self.path_channel = Some(rx);
                            let settings = self.path_settings.clone();
                            let data = data.clone();
                            let ctx = ContextUpdater::new(ui.ctx());
                            thread::spawn(move || {
                                let data = (*data.read()).clone();
                                Self::do_pathfinding(settings, &data, tx, ctx);
                            });
                            Some(PathStatus::Loading)
                        }
                    }
                }

                if let Some(st) = &self.path_status {
                    use PathStatus::*;
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
                                    set_bg_color_tinted(Color32::RED, ui);
                                    self.person_button(&data, ui, id, &mut cur_path);
                                    if i != 0 && i != path.len() - 1 && ui.button("x").clicked() {
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

#[derive(Default)]
pub struct ClassSection {}

#[derive(Derivative)]
#[derivative(Default)]
pub struct InfosSection {
    pub infos_current: Option<usize>,
    pub infos_open: bool,
    #[derivative(Default(value = "1"))]
    pub neighborhood_degree: usize,
    pub paradox: ParadoxState,
}

#[derive(Default)]
struct ParadoxState {
    current: Option<usize>,
    sum: usize,
    min: usize,
    max: usize,
}

impl InfosSection {
    fn set_infos_current(&mut self, id: Option<usize>) {
        self.infos_current = id;
        self.infos_open = id.is_some();
    }

    fn show(
        &mut self,
        data_rw: &Arc<MyRwLock<ViewerData>>,
        tab_request: &mut Option<NewTabRequest>,
        ui: &mut Ui,
        camera: &Camera,
        path_section: &PathSection,
        sel_field: &mut SelectedUserField,
        modal: &impl ModalWriter,
    ) {
        CollapsingHeader::new("Informations")
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    set_bg_color_tinted(Color32::GREEN, ui);
                    ui.radio_value(sel_field, SelectedUserField::Selected, "");
                    combo_with_filter(ui, "#infos_user", &mut self.infos_current, data_rw);
                });
                if let Some(id) = self.infos_current {
                    let data = &*data_rw.read();
                    let person = &data.persons[id];
                    let class = person.modularity_class;

                    egui::Grid::new("#infos").show(ui, |ui| {
                        ui.label("ID Facebook :");
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
                        ui.label("Amis :");
                        ui.label(format!("{}", person.neighbors.len()));
                        ui.end_row();
                        ui.label("Classe :");
                        ui.horizontal(|ui| {
                            ClassSection::class_circle(ui, &data.modularity_classes[class as usize]);
                            self.create_class_subgraph(data_rw, tab_request, camera, path_section, modal, class, ui);
                        });
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

                    CollapsingHeader::new("Paradoxe de l'amitié")
                        .default_open(false)
                        .show(ui, |ui| {
                            if self.paradox.current != self.infos_current {
                                let mut sum = 0;
                                let friends = person.neighbors.iter()
                                    .map(|&i| data.persons[i].neighbors.len())
                                    .inspect(|n| sum += n)
                                    .minmax();
                                use MinMaxResult::*;
                                let (min, max) = match friends {
                                    NoElements => (0, 0),
                                    OneElement(n) => (n, n),
                                    MinMax(min, max) => (min, max),
                                };
                                self.paradox = ParadoxState { current: Some(id), sum, min, max };
                            }

                            let state = &self.paradox;

                            egui::Grid::new("#paradox").show(ui, |ui| {
                                ui.label("Amis :");
                                ui.label(format!("{}", person.neighbors.len()));
                                ui.end_row();
                                ui.label("Amis de mes amis (moy) :");
                                ui.label(format!("{}", state.sum / person.neighbors.len()));
                                ui.end_row();
                                ui.label("Amis de mes amis (min) :");
                                ui.label(format!("{}", state.min));
                                ui.end_row();
                                ui.label("Amis de mes amis (max) :");
                                ui.label(format!("{}", state.max));
                                ui.end_row();
                            });
                        });

                    ui.horizontal(|ui| {
                        ui.style_mut().spacing.slider_width = 100.0;
                        ui.add(
                            egui::Slider::new(&mut self.neighborhood_degree, 1..=13)
                                .text("Degré")
                                .clamping(SliderClamping::Always),
                        );

                        if ui.button("Afficher voisinage")
                            .on_hover_text("Afficher les amis jusqu'à une certaine distance de la personne. Le degré 1 affichera les amis directs, le degré 2 les amis des amis, etc.")
                            .clicked() {
                            let neighborhood_degree = self.neighborhood_degree;
                            self.create_subgraph(
                                format!("{}-voisinage de {}", neighborhood_degree, person.name),
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
                                            log!(status_tx, "No new friends at degree {}", i + 1);
                                            if last_batch.len() < 50 {
                                                log!(status_tx, "At {}: {:?}", i, last_batch.iter().map(|i| data.persons[*i].name).collect::<Vec<_>>());
                                            }
                                            break;
                                        }
                                        new_included.extend(new_friends.iter().copied());
                                        log!(status_tx, "{} new friends at degree {}", new_friends.len(), i + 1);
                                        last_batch = new_friends;
                                    }

                                    log!(status_tx, "Got {} persons total", new_included.len());
                                    Ok(new_included)
                                });
                        }
                    });
                }
            });
    }

    fn create_class_subgraph(&self,
                             data_rw: &Arc<MyRwLock<ViewerData>>,
                             tab_request: &mut Option<NewTabRequest>,
                             camera: &Camera,
                             path_section: &PathSection,
                             modal: &impl ModalWriter,
                             class: u16,
                             ui: &mut Ui) {
        if ui.button(format!("{}", class)).clicked() {
            self.create_subgraph(
                format!("Classe {}", class),
                data_rw, tab_request, camera, path_section, ui, modal.clone(),
                move |_, data| {
                    Ok(data.persons
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| p.modularity_class == class)
                        .map(|(i, _)| i)
                        .collect())
                });
        }
    }

    fn create_subgraph(&self,
                       title: String,
                       data: &Arc<MyRwLock<ViewerData>>,
                       tab_request: &mut Option<NewTabRequest>,
                       camera: &Camera,
                       path_section: &PathSection,
                       ui: &mut Ui,
                       modal_tx: impl ModalWriter,
                       x: impl FnOnce(&StatusWriter, &ViewerData) -> Cancelable<AHashSet<usize>> + Send + 'static) {
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

            let mut new_persons =
                Vec::with_capacity(new_included.len());

            let mut id_map = AHashMap::new();
            let mut class_list = AHashSet::new();

            log!(status_tx, "Processing person list and creating ID map");
            {
                let data = data.read();
                for &id in new_included.iter() {
                    let pers = &data.persons[id];
                    id_map.insert(id, new_persons.len());
                    class_list.insert(pers.modularity_class);
                    new_persons.push(Person {
                        neighbors: vec![],
                        ..*pers
                    });
                }
            }

            let mut edges = Vec::new();

            log!(status_tx, "Creating new neighbor lists and edge list");
            {
                let data = data.read();
                for_progress!(status_tx, (&old_id, &new_id) in id_map.iter(), {
                    new_persons[new_id].neighbors.extend(
                        data.persons[old_id]
                            .neighbors
                            .iter()
                            .filter_map(|&i| id_map.get(&i)),
                    );
                    for &nb in new_persons[new_id].neighbors.iter() {
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
            }

            log!(status_tx, "Computing min edge filter");

            let mut filter = 1;
            const MAX: usize = 10000;
            while new_persons.iter()
                .filter(|p| p.neighbors.len() as u16 >= filter)
                .enumerate().any(|(i, _)| i >= MAX) { // count() would iterate all the nodes
                filter += 1;
            }

            let viewer = ViewerData::new(new_persons, data.read().modularity_classes.clone(), &status_tx)?;

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

#[derive(Derivative)]
#[derivative(Default)]
pub struct DetailsSection {
    pub mouse_pos: Option<Pos2>,
    pub mouse_pos_world: Option<Vector2<f32>>,
}

impl DetailsSection {
    fn show(&mut self, ui: &mut Ui, camera: &mut TabCamera, cid: Id) {
        CollapsingHeader::new("Détails")
            .default_open(false)
            .show(ui, |ui| {
                let trans = &camera.camera.transf;
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
                    ui.label("Échelle :");
                    ui.label(format!("{:.3}", trans.scaling()));
                    ui.end_row();
                    ui.label("Angle :");
                    ui.label(format!("{:.3}", trans.isometry.rotation.angle()));
                    ui.end_row();
                    ui.label("Translation :");
                    let offs = trans.isometry.translation;
                    ui.label(format!("({:.3}, {:.3})", offs.x, offs.y));
                    ui.end_row();
                });
                if ui.button("Réinitialiser caméra").clicked() {
                    camera.camera = camera.camera_default;
                }
                if ui.button("Centrer caméra").clicked() {
                    ui.ctx().animate_bool_with_time(cid, true, 0.0);
                    camera.cam_animating = Some(CamAnimating::PanTo { from: camera.camera.transf, to: camera.camera_default.transf });
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

pub struct ForceAtlasState {
    running: bool,
    data: Option<(Arc<RwLock<Layout<f32, 2>>>, Option<ForceAtlasThread>)>,
    settings: Settings<f32>,
    new_settings: Arc<(AtomicBool, Mutex<Settings<f32>>)>,
    render_thread: Option<(Sender<()>, Receiver<GlTask>, JoinHandle<()>)>,
}

impl Default for ForceAtlasState {
    fn default() -> Self {
        Self {
            running: false,
            data: None,
            settings: Settings {
                theta: 0.5,
                ka: 0.1,
                kg: 0.1,
                kr: 0.02,
                lin_log: false,
                speed: 0.01,
                prevent_overlapping: None,
                strong_gravity: false,
            },
            new_settings: Default::default(),
            render_thread: None,
        }
    }
}

#[derive(Default)]
pub struct AlgosSection {
    //algo_task: Option<Box<dyn FnOnce(&UiState) + 'static>>,
    louvain_precision: f32,
    louvain_state: Option<LouvainState>,
    force_atlas_state: ForceAtlasState,
    times: Vec<Duration>,
}

pub struct LouvainState {
    thread: JoinHandle<()>,
    status_rx: StatusReader,
    //data_rx: Receiver<()>,
    //status_tx: Sender<LouvainStatus>,
}

pub struct ForceAtlasThread {
    thread: JoinHandle<()>,
    status_tx: Sender<bool>,
}

fn rerender_graph(data: &ViewerData) -> GlTask {
    let nodes = data
        .persons
        .iter()
        .map(|p| {
            crate::geom_draw::create_node_vertex(p)
        });

    let edges = data.get_edges().flat_map(
        |(a, b)| crate::geom_draw::create_edge_vertices(&data.persons[a], &data.persons[b])
    );
    let vertices = nodes.chain(edges).collect_vec();

    let closure = move |graph: &mut RenderedGraph, gl: &glow::Context| unsafe {
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(graph.nodes_buffer));
        gl.buffer_sub_data_u8_slice(
            glow::ARRAY_BUFFER,
            0,
            std::slice::from_raw_parts(
                vertices.as_ptr() as *const u8,
                vertices.len() * size_of::<PersonVertex>(),
            ),
        );
    };

    Box::new(closure)
}

impl AlgosSection {
    fn show(&mut self,
            data: &Arc<MyRwLock<ViewerData>>,
            ui: &mut Ui,
            graph: &Arc<MyRwLock<RenderedGraph>>,
            stats: &Arc<MyRwLock<NodeStats>>) {
        CollapsingHeader::new("Algorithmes")
            .default_open(false)
            .show(ui, |ui| {
                if ui.add_enabled(self.louvain_state.is_none(), egui::Button::new("Louvain")).clicked() {
                    let (status_tx, status_rx) = status_pipe(ui.ctx());
                    let data = data.clone();
                    let graph = graph.clone();
                    const ITERATIONS: usize = 100;
                    let precision = self.louvain_precision;
                    let stats = stats.clone();
                    let thr = thread::spawn(move || {
                        let data_ = data.read();
                        let mut louvain = crate::algorithms::louvain::Graph::new(&data_.persons);
                        for i in 0..ITERATIONS {
                            if try_log_progress!(status_tx, i, ITERATIONS).is_err() {
                                return;
                            }
                            let old_stats = louvain.stats();
                            louvain = louvain.next(precision);
                            let new_stats = louvain.stats();
                            if old_stats == new_stats {
                                break;
                            }
                        }
                        if try_log_progress!(status_tx, ITERATIONS, ITERATIONS).is_err() {
                            return;
                        }
                        let mut nodes = data_.persons.clone();
                        for n in &mut nodes {
                            n.modularity_class = u16::MAX;
                        }
                        drop(data_);

                        use colourado_iter::{ColorPalette, PaletteType};
                        let palette = ColorPalette::new(PaletteType::Random, false, &mut rand::thread_rng());
                        let mut classes = Vec::new();
                        println!("result: {} classes", louvain.nodes.len());

                        for (i, (comm, color)) in louvain.nodes.iter().zip(palette).enumerate() {
                            for user in comm.payload.as_ref().unwrap() {
                                nodes[user.0].modularity_class = i as u16;
                            }
                            let [r, g, b] = color.to_array();
                            classes.push(ModularityClass::new(Color3b {
                                r: (r * 255.0) as u8,
                                g: (g * 255.0) as u8,
                                b: (b * 255.0) as u8,
                            }, (i + 1) as u16));
                        }

                        {
                            let mut lock = data.write();
                            lock.persons = nodes;
                            lock.modularity_classes = classes;

                            *stats.write() = NodeStats::new(&lock, graph.read().node_filter);
                        }

                        graph.write().tasks.push_back(rerender_graph(&data.read()));
                    });
                    self.louvain_state = Some(LouvainState {
                        thread: thr,
                        status_rx,
                    });
                }

                if let Some(ref mut state) = self.louvain_state {
                    if state.thread.is_finished() {
                        self.louvain_state = None;
                    } else {
                        state.status_rx.recv();
                        if ui.horizontal(|ui| {
                            ui.spinner();
                            let cancel = ui.button("🗙").clicked();
                            show_progress_bar(ui, &state.status_rx);
                            cancel
                        }).inner {
                            self.louvain_state = None;
                        };
                    }
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Précision :");
                        ui.add(egui::Slider::new(&mut self.louvain_precision, 1e-7..=1.0)
                            .logarithmic(true)
                            .custom_formatter(|n, _| format!("{:.1e}", n))
                            .text("")).changed();
                    });
                }

                if ui.checkbox(&mut self.force_atlas_state.running, "ForceAtlas2").changed() {
                    if let Some((_, Some(thr))) = &self.force_atlas_state.data {
                        thr.status_tx.send(self.force_atlas_state.running).expect("Failed to send pause signal");
                    }
                }

                egui::Grid::new("#forceatlas").show(ui, |ui| {
                    let mut upd = false;

                    // TODO: better ranges for these
                    // TODO: presets?
                    let fields = [
                        ("Theta", &mut self.force_atlas_state.settings.theta),
                        ("Ka", &mut self.force_atlas_state.settings.ka),
                        ("Kg", &mut self.force_atlas_state.settings.kg),
                        ("Kr", &mut self.force_atlas_state.settings.kr),
                        ("Speed", &mut self.force_atlas_state.settings.speed),
                    ];

                    for (name, field) in fields.into_iter() {
                        ui.label(name);
                        upd |= ui.add(egui::Slider::new(field, 0.001..=10.0).logarithmic(true).text("")).changed();
                        ui.end_row();
                    }

                    ui.label("Lin-Log");
                    upd |= ui.checkbox(&mut self.force_atlas_state.settings.lin_log, "").changed();
                    ui.end_row();

                    ui.label("Strong gravity");
                    upd |= ui.checkbox(&mut self.force_atlas_state.settings.strong_gravity, "").changed();
                    ui.end_row();

                    if upd {
                        *self.force_atlas_state.new_settings.1.lock() = self.force_atlas_state.settings.clone();
                        self.force_atlas_state.new_settings.0.store(true, std::sync::atomic::Ordering::Release);
                    }
                });

                if self.force_atlas_state.running {
                    ui.spinner();

                    let layout = self.force_atlas_state.data.get_or_insert_with(|| {
                        const UPD_PER_SEC: usize = 60;

                        let data = data.read();
                        let layout = Arc::new(RwLock::new(Layout::<f32, 2>::from_positioned(
                            self.force_atlas_state.settings.clone(),
                            data.persons.iter().map(|p| Node {
                                pos: VecN(p.position.to_array()),
                                ..Default::default()
                            }).collect(),
                            data.get_edges().map(|e| (e, 1.0)).collect(),
                        )));
                        let (status_tx, status_rx) = mpsc::channel();
                        let layout_thr = layout.clone();
                        let settings_thr = self.force_atlas_state.new_settings.clone();

                        let thread = thread::spawn(move || {
                            loop {
                                loop {
                                    {
                                        let mut layout = layout_thr.write();

                                        layout.iteration();

                                        if settings_thr.0.load(std::sync::atomic::Ordering::Acquire) {
                                            layout.set_settings(settings_thr.1.lock().clone());
                                            settings_thr.0.store(false, std::sync::atomic::Ordering::Release);
                                        }
                                    }

                                    // check if the layout has been paused
                                    match status_rx.try_recv() {
                                        Ok(true) => {} // continue
                                        Ok(false) => break, // pause
                                        Err(TryRecvError::Empty) => {} // no change
                                        Err(TryRecvError::Disconnected) => return, // tab closed
                                    }

                                    thread::sleep(Duration::from_secs_f32(1.0 / UPD_PER_SEC as f32));
                                }
                                loop {
                                    // wait for resume
                                    match status_rx.recv() {
                                        Ok(true) => break, // resume
                                        Ok(false) => {} // keep paused
                                        Err(RecvError) => return, // tab closed
                                    }
                                }
                            }
                        });
                        (layout, Some(ForceAtlasThread { thread, status_tx }))
                    }).0.clone();

                    let (s, r, t) = self.force_atlas_state.render_thread.get_or_insert_with(|| {
                        let (request_tx, request_rx) = mpsc::channel();
                        let (result_tx, result_rx) = mpsc::channel();
                        let thr_data = data.clone();
                        request_tx.send(()).unwrap();
                        let graph = graph.clone();
                        let stats = stats.clone();
                        (request_tx, result_rx, thread::spawn(move || {
                            while let Ok(()) = request_rx.recv() {
                                let mut persons = thr_data.read().persons.clone();
                                for (person, node) in persons.iter_mut().zip(layout.read().nodes.iter()) {
                                    person.position = Point::new(node.pos[0], node.pos[1]);
                                }

                                {
                                    let mut data_w = thr_data.write();
                                    data_w.persons = persons;

                                    *stats.write() = NodeStats::new(&data_w, graph.read().node_filter);
                                }

                                let closure = rerender_graph(&thr_data.read());
                                if result_tx.send(closure).is_err() {
                                    return; // tab closed
                                }
                            }
                        }))
                    });

                    if let Ok((task)) = r.try_recv() {
                        graph.write().tasks.push_back(task);
                        s.send(()).unwrap();
                    }
                }
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
pub struct NodeStats {
    node_count: usize,
    node_classes: Vec<(usize, usize)>,
}

impl NodeStats {
    pub fn new(data: &ViewerData, filter: NodeFilter) -> Self {
        let mut count_classes = vec![0; data.modularity_classes.len()];
        let mut node_count = 0;
        for p in &*data.persons {
            let ok = if filter.filter_nodes {
                let deg = p.neighbors.len() as u16;
                deg >= filter.degree_filter.0 && deg <= filter.degree_filter.1
            } else {
                true
            };
            if ok {
                node_count += 1;
                count_classes[p.modularity_class as usize] += 1;
            }
        }
        let node_classes = count_classes
            .iter()
            .enumerate()
            .filter(|(_, &c)| c != 0)
            .sorted_by_key(|(_, &c)| std::cmp::Reverse(c))
            .map(|(i, &c)| (i, c))
            .collect_vec();
        Self {
            node_count,
            node_classes,
        }
    }
}

#[derive(Default)]
pub struct UiState {
    pub display: DisplaySection,
    pub path: PathSection,
    pub classes: ClassSection,
    pub infos: InfosSection,
    pub details: DetailsSection,
    pub selected_user_field: SelectedUserField,
    pub algorithms: AlgosSection,

    pub stats: Arc<MyRwLock<NodeStats>>,
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
    fn show(
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
        CollapsingHeader::new(format!("Classes ({})", stats.read().node_classes.len()))
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

    fn class_circle(ui: &mut Ui, cl: &ModularityClass) {
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
                            .clamping(SliderClamping::Always),
                    );
                }
                ui.checkbox(&mut self.g_show_edges, "Afficher les liens");
                if self.g_show_edges {
                    ui.add(
                        egui::Slider::new(&mut self.g_opac_edges, 0.0..=1.0)
                            .text("Opacité")
                            .custom_formatter(percent_formatter)
                            .custom_parser(percent_parser)
                            .clamping(SliderClamping::Always),
                    );
                }

                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        let start = ui
                            .add(
                                egui::DragValue::new(&mut graph.node_filter.degree_filter.0)
                                    .speed(1)
                                    .range(1..=graph.node_filter.degree_filter.1)
                                    .prefix("Degré minimum : "),
                            )
                            .changed();
                        let end = ui
                            .add(
                                egui::DragValue::new(&mut graph.node_filter.degree_filter.1)
                                    .speed(1)
                                    .range(graph.node_filter.degree_filter.0..=self.max_degree)
                                    .prefix("Degré maximum : "),
                            )
                            .changed();
                        if start || end {
                            self.deg_filter_changed = true;
                        }
                    });
                    ui.vertical(|ui| {
                        ui.checkbox(&mut graph.node_filter.filter_nodes, "Filtrer les nœuds");
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
    pub fn draw_ui(
        &mut self,
        ui: &mut Ui,
        data: &Arc<MyRwLock<ViewerData>>,
        graph: &Arc<MyRwLock<RenderedGraph>>,
        tab_request: &mut Option<NewTabRequest>,
        camera: &mut TabCamera,
        cid: Id,
        modal: &impl ModalWriter,
    ) {
        ui.spacing_mut().slider_width = 200.0;
        egui::ScrollArea::vertical().show(ui, |ui| {
            self.display.show(&mut graph.write(), ui);

            if self.display.deg_filter_changed {
                *self.stats.write() = NodeStats::new(&data.read(), graph.read().node_filter);
            }

            self.path.show(
                data,
                ui,
                &mut self.infos,
                &mut self.selected_user_field,
            );

            self.infos.show(
                data,
                tab_request,
                ui,
                &camera.camera,
                &self.path,
                &mut self.selected_user_field,
                modal,
            );

            self.classes.show(
                ui,
                &self.infos,
                data, tab_request,
                &camera.camera,
                &self.path,
                modal,
                &self.stats,
            );

            self.algorithms.show(data, ui, graph, &self.stats);

            self.details.show(ui, camera, cid);
        });
    }
}
