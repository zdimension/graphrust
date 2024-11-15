use crate::algorithms::AbstractGraph;
use crate::app::{show_progress_bar, ViewerData};
use crate::graph_render::RenderedGraph;
use crate::thread::JoinHandle;
use crate::threading::{spawn_cancelable, status_pipe, CancelableError, MyRwLock, StatusReader};
use crate::ui;
use crate::ui::modal::{ModalInfo, ModalWriter};
use crate::ui::NodeStats;
use crate::{log_progress, thread};
use egui::{CollapsingHeader, Ui};
use forceatlas2::{Layout, Node, Settings, VecN};
use graph_format::Point;
use parking_lot::{Mutex, RwLock};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, RecvError, Sender, TryRecvError};
use std::sync::{mpsc, Arc};
use std::time::Duration;

pub struct ForceAtlasRenderDone;

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

impl AlgosSection {
    pub(crate) fn show(&mut self,
                       data: &Arc<MyRwLock<ViewerData>>,
                       ui: &mut Ui,
                       graph: &Arc<MyRwLock<RenderedGraph>>,
                       stats: &Arc<MyRwLock<NodeStats>>,
                       modal: &impl ModalWriter) {
        CollapsingHeader::new(t!("Algorithms"))
            .default_open(false)
            .show(ui, |ui| {
                if data.read().persons.len() > 50_000 {
                    ui.label(t!("large_graph_warning"));
                }
                if ui.add_enabled(self.louvain_state.is_none(), egui::Button::new("Louvain")).clicked() {
                    let (status_tx, status_rx) = status_pipe(ui.ctx());
                    let data = data.clone();
                    let graph = graph.clone();
                    const ITERATIONS: usize = 100;
                    let precision = self.louvain_precision;
                    let stats = stats.clone();
                    let thr = spawn_cancelable(modal.clone(), move || {
                        let mut louvain = crate::algorithms::louvain::Graph::new(&data.read().persons);
                        for i in 0..ITERATIONS {
                            log_progress!(status_tx, i, ITERATIONS);
                            let old_stats = louvain.stats();
                            louvain = louvain.next(precision);
                            let new_stats = louvain.stats();
                            if old_stats == new_stats {
                                break;
                            }
                        }
                        log_progress!(status_tx, ITERATIONS, ITERATIONS);
                        if louvain.nodes.len() > RenderedGraph::MAX_RENDER_CLASSES {
                            return Err(CancelableError::Custom(ModalInfo {
                                title: t!("Too many classes").to_string(),
                                body: t!("Too many classes (%{len}) to display the result. The current limit is %{max}.\n\nTry decreasing the precision.", len = louvain.nodes.len(), max = RenderedGraph::MAX_RENDER_CLASSES).into(),
                            }.into()));
                        }

                        let data_ = data.read();
                        let mut nodes = data_.persons.as_ref().clone();
                        for n in &mut nodes {
                            n.modularity_class = u16::MAX;
                        }
                        drop(data_);

                        use colourado_iter::{ColorPalette, PaletteType};
                        use graph_format::Color3b;
                        use crate::app::ModularityClass;
                        use crate::ui;
                        let palette = ColorPalette::new(PaletteType::Random, false, &mut rand::thread_rng());
                        let mut classes = Vec::new();

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

                        let task = ui::rerender_graph(&nodes);

                        {
                            let mut lock = data.write();
                            lock.persons = Arc::new(nodes);
                            lock.modularity_classes = classes;

                            let mut graph = graph.write();
                            *stats.write() = NodeStats::new(&lock, graph.node_filter);
                            graph.tasks.push_back(task);
                        }

                        Ok(())
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
                            let cancel = ui.button("✖").clicked();
                            show_progress_bar(ui, &state.status_rx);
                            cancel
                        }).inner {
                            self.louvain_state = None;
                        };
                    }
                } else {
                    ui.horizontal(|ui| {
                        ui.label(t!("Precision:"));
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
                        (t!("Theta"), &mut self.force_atlas_state.settings.theta),
                        (t!("Ka"), &mut self.force_atlas_state.settings.ka),
                        (t!("Kg"), &mut self.force_atlas_state.settings.kg),
                        (t!("Kr"), &mut self.force_atlas_state.settings.kr),
                        (t!("Speed"), &mut self.force_atlas_state.settings.speed),
                    ];

                    for (name, field) in fields.into_iter() {
                        ui.label(name);
                        upd |= ui.add(egui::Slider::new(field, 0.001..=10.0).logarithmic(true).text("")).changed();
                        ui.end_row();
                    }

                    ui.label(t!("Lin-Log"));
                    upd |= ui.checkbox(&mut self.force_atlas_state.settings.lin_log, "").changed();
                    ui.end_row();

                    ui.label(t!("Strong gravity"));
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
                            data.persons.iter().get_edges().map(|e| (e, 1.0)).collect(),
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

                    let (s, r, _t) = self.force_atlas_state.render_thread.get_or_insert_with(|| {
                        let (request_tx, request_rx) = mpsc::channel();
                        let (result_tx, result_rx) = mpsc::channel();
                        let thr_data = data.clone();
                        request_tx.send(()).unwrap();
                        let graph = graph.clone();
                        let stats = stats.clone();
                        (request_tx, result_rx, thread::spawn(move || {
                            while let Ok(()) = request_rx.recv() {
                                let mut persons = thr_data.read().persons.as_ref().clone();
                                for (person, node) in persons.iter_mut().zip(layout.read().nodes.iter()) {
                                    person.position = Point::new(node.pos[0], node.pos[1]);
                                }

                                let closure = ui::rerender_graph(&persons);

                                {
                                    let mut data_w = thr_data.write();
                                    data_w.persons = Arc::new(persons);

                                    let mut graph = graph.write();
                                    *stats.write() = NodeStats::new(&data_w, graph.node_filter);
                                    graph.tasks.push_back(closure);
                                }
                                if result_tx.send(ForceAtlasRenderDone).is_err() {
                                    return; // tab closed
                                }
                            }
                        }))
                    });

                    if let Ok(ForceAtlasRenderDone) = r.try_recv() {
                        s.send(()).unwrap();
                    }
                }
            });
    }
}

pub struct ForceAtlasState {
    running: bool,
    data: Option<(Arc<RwLock<Layout<f32, 2>>>, Option<ForceAtlasThread>)>,
    settings: Settings<f32>,
    new_settings: Arc<(AtomicBool, Mutex<Settings<f32>>)>,
    render_thread: Option<(Sender<()>, Receiver<ForceAtlasRenderDone>, JoinHandle<()>)>,
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