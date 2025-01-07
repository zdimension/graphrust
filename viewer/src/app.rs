use crate::graph_render::camera::{CamXform, Camera};
use crate::graph_storage::{load_binary, load_file, ProcessedData};
use crate::ui::{tabs, UiState};
use eframe::glow::HasContext;
use eframe::{egui_glow, glow};
use egui::{
    vec2, CentralPanel, Color32, Context, FontFamily, FontId, Frame, Hyperlink, Id, Layout,
    RichText, TextFormat, TextStyle, Ui, Vec2, WidgetText,
};
use egui_dock::{DockArea, DockState, Style};
use graph_format::{Color3b, Point};

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use zearch::{Document, Index, Search};

use crate::graph_render::{GlForwarder, GlMpsc};
use crate::search::SearchEngine;
use crate::threading;
use crate::threading::{Cancelable, StatusReader, StatusWriter, StatusWriterInterface};
use crate::ui::modal::{show_modal, ModalInfo};
use crate::ui::tabs::{GraphTab, GraphTabLoaded, TabViewer};
use eframe::emath::Align;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
#[cfg(not(target_arch = "wasm32"))]
pub use std::thread;
#[cfg(target_arch = "wasm32")]
pub use wasm_thread as thread;

#[macro_export]
macro_rules! log {
    ($ch:expr, $fmt:literal, $($arg:tt)+) => {
        {
            use $crate::threading::StatusWriterInterface;
            let msg = format!($fmt, $($arg)+);
            log::info!("{}", &msg);
            $ch.send(msg.clone())?;
        }
    };
    ($ch:expr, $e: expr) => {
        $crate::log!($ch, "{}", $e)
    }
}

#[macro_export]
macro_rules! try_log_progress {
    ($ch: expr, $val:expr, $max:expr) => {{
        use $crate::threading::StatusWriterInterface;
        $ch.send($crate::threading::Progress {
            max: $max,
            val: $val,
        })
    }};
}

#[macro_export]
macro_rules! log_progress {
    ($( $arg:expr ),*) => {
        $crate::try_log_progress!($( $arg ),*)?;
    }
}

#[macro_export]
macro_rules! for_progress {
    ($ch:expr, $var:pat in $iter:expr, $block:block) => {{
        let max = ExactSizeIterator::len(&$iter);
        let how_often = (max / 100).max(1);
        for (i_, $var) in $iter.enumerate() {
            $block;
            if i_ % how_often == 0 {
                $crate::log_progress!($ch, i_, max);
            }
        }
    }};
}

pub fn iter_progress<'a, T>(
    iter: T,
    ch: &'a impl StatusWriterInterface,
) -> impl Iterator<Item = T::Item> + 'a
where
    T: ExactSizeIterator + 'a,
{
    let max = ExactSizeIterator::len(&iter);
    let how_often = (max / 100).max(1);
    iter.enumerate().map(move |(i, x)| {
        if i % how_often == 0 {
            let _ = try_log_progress!(ch, i, max);
        }
        x
    })
}

#[derive(Clone)]
pub struct Person {
    pub position: Point,
    pub size: f32,
    pub modularity_class: u16,
    pub id: &'static str,
    pub name: &'static str,
    pub neighbors: Vec<usize>,
}

impl Person {
    pub fn new(
        position: Point,
        size: f32,
        modularity_class: u16,
        id: &'static str,
        name: &'static str,
        total_edge_count: usize,
    ) -> Person {
        Person {
            position,
            size,
            modularity_class,
            id,
            name,
            neighbors: Vec::with_capacity(total_edge_count),
        }
    }
}

#[derive(Clone)]
pub struct ModularityClass {
    pub color: Color3b,
    pub id: u16,
    pub name: String,
}

impl ModularityClass {
    pub fn new(color: Color3b, id: u16) -> ModularityClass {
        ModularityClass {
            color,
            id,
            name: format!("Classe {}", id),
        }
    }
}

//#[derive(Clone)]
pub struct ViewerData {
    pub persons: Arc<Vec<Person>>,
    pub modularity_classes: Vec<ModularityClass>,
    pub engine: Arc<SearchEngine>,
}

impl ViewerData {
    pub fn new(
        persons: Vec<Person>,
        modularity_classes: Vec<ModularityClass>,
    ) -> Cancelable<ViewerData> {
        let persons = Arc::new(persons);
        let engine = Arc::new(SearchEngine::new(persons.clone()));
        Ok(ViewerData {
            persons,
            modularity_classes,
            engine,
        })
    }
}

pub struct StringTables {
    pub ids: Vec<u8>,
    pub names: Vec<u8>,
}

pub enum GraphTabState {
    Loading {
        status_rx: StatusReader,
        state_rx: Receiver<GraphTabLoaded>,
        gl_mpsc: GlMpsc,
    },
    Loaded(GraphTabLoaded),
}

impl GraphTabState {
    pub fn loading(
        status_rx: StatusReader,
        state_rx: Receiver<GraphTabLoaded>,
        gl_mpsc: GlMpsc,
    ) -> Self {
        GraphTabState::Loading {
            status_rx,
            state_rx,
            gl_mpsc,
        }
    }
}

#[derive(Clone)]
pub struct ContextUpdater {
    #[cfg(target_arch = "wasm32")]
    tx_ctx: tokio::sync::mpsc::UnboundedSender<()>,
    #[cfg(not(target_arch = "wasm32"))]
    ctx: Context,
}

impl ContextUpdater {
    pub fn new(ctx: &Context) -> Self {
        let ctx = ctx.clone();
        #[cfg(target_arch = "wasm32")]
        {
            let (tx_ctx, mut rx_ctx) = tokio::sync::mpsc::unbounded_channel();
            wasm_bindgen_futures::spawn_local(async move {
                loop {
                    let Some(()) = rx_ctx.recv().await else {
                        break;
                    };
                    ctx.request_repaint();
                }
            });
            ContextUpdater { tx_ctx }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            ContextUpdater { ctx }
        }
    }

    pub fn update(&self) {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = self.tx_ctx.send(());
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.ctx.request_repaint();
        }
    }
}

pub struct GraphViewApp {
    top_bar: bool,
    tasks: Receiver<EguiTask>,
    modal: (Receiver<ModalInfo>, Sender<ModalInfo>),
    state: AppState,
    md_cache: CommonMarkCache,
}

pub enum AppState {
    Loading {
        status_rx: StatusReader,
        file_rx: Receiver<ProcessedData>,
    },
    Loaded {
        tree: DockState<GraphTab>,
        #[allow(dead_code)]
        /// we do a little trolling
        ///
        /// this is for keeping the StringTables object allocated since the graph objects have
        /// `&'static str`s pointing to it
        string_tables: StringTables,
    },
}

pub type EguiTask = Box<dyn FnOnce(&Context) + Send>;

impl GraphViewApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.style_mut(|s| s.animation_time = 1.0 / 6.0);

        let gl = cc
            .gl
            .as_ref()
            .expect("You need to run eframe with the glow backend");
        // SAFETY: duh
        unsafe {
            gl.enable(glow::PROGRAM_POINT_SIZE);
        }

        let (status_tx, status_rx) = threading::status_pipe(&cc.egui_ctx);
        let (file_tx, file_rx) = mpsc::channel();
        let (modal_tx, modal_rx) = mpsc::channel();
        let (ctx_tx, ctx_rx) = mpsc::channel();

        threading::spawn_cancelable(modal_tx.clone(), move || {
            let res: Result<_, anyhow::Error> = try {
                let font = crate::http::download_bytes("fonts/noto_sans_arabic.ttf")?;
                let task: EguiTask = Box::new(move |ctx: &Context| {
                    let mut fonts = egui::FontDefinitions::default();
                    let name = "Noto Sans Arabic";
                    fonts
                        .font_data
                        .insert(name.to_string(), egui::FontData::from_owned(font).into());
                    fonts
                        .families
                        .entry(FontFamily::Proportional)
                        .or_default()
                        .push(name.to_string());
                    ctx.set_fonts(fonts);
                    log::info!("Arabic font loaded");
                });
                ctx_tx.send(task)
            };
            if let Err(e) = res {
                log::error!("Error loading Arabic font: {}", e);
            }
            Ok(())
        });

        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let Ok(res) = load_file(&status_tx).await else {
                log::info!("Error loading graph file");
                return;
            };

            thread::spawn(move || {
                let Ok(res) = load_binary(&status_tx, res) else {
                    log::info!("Error processing graph file");
                    return;
                };
                file_tx.send(res).unwrap();
            });
        });

        #[cfg(not(target_arch = "wasm32"))]
        threading::spawn_cancelable(modal_tx.clone(), move || {
            let res = load_file(&status_tx)?;
            let res = load_binary(&status_tx, res)?;
            file_tx.send(res)?;
            Ok(())
        });

        Self {
            top_bar: true,
            modal: (modal_rx, modal_tx),
            tasks: ctx_rx,
            state: AppState::Loading { status_rx, file_rx },
            md_cache: CommonMarkCache::default(),
        }
    }
}

pub(crate) fn show_status(ui: &mut Ui, status_rx: &mut StatusReader) {
    ui.vertical_centered(|ui| {
        ui.spinner();
        ui.label(status_rx.recv());
        show_progress_bar(ui, status_rx);
    });
}

pub fn show_progress_bar(ui: &mut Ui, status_rx: &StatusReader) {
    if let Some(p) = status_rx.progress {
        ui.add(
            egui::ProgressBar::new(p.val as f32 / p.max as f32)
                .desired_height(12.0)
                .desired_width(230.0),
        );
    }
}

impl eframe::App for GraphViewApp {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
        let mut new_tab_request = None;

        while let Ok(task) = self.tasks.try_recv() {
            task(ctx);
        }

        self.show_top_bar(ctx, self.top_bar);

        show_modal(ctx, &self.modal.0, "modal");

        CentralPanel::default()
            .frame(Frame::central_panel(&ctx.style()).inner_margin(0.))
            .show(ctx, |ui| {
                match &mut self.state {
                    AppState::Loading { status_rx, file_rx } => {
                        show_status(ui, status_rx);
                        if let Ok(file) = file_rx.try_recv() {
                            let (status_tx, status_rx) = threading::status_pipe(ctx);
                            let (state_tx, state_rx) = mpsc::channel();
                            let (gl_fwd, gl_mpsc) = GlForwarder::new();
                            self.state = AppState::Loaded {
                                tree: DockState::new(vec![GraphTab {
                                    id: Id::new(("main_tab", chrono::Utc::now())),
                                    closeable: false,
                                    title: t!("Graph").to_string(),
                                    state: GraphTabState::loading(status_rx, state_rx, gl_mpsc),
                                }]),
                                string_tables: file.strings,
                            };
                            threading::spawn_cancelable(self.modal.1.clone(), move || {
                                let mut min = Point::new(f32::INFINITY, f32::INFINITY);
                                let mut max = Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
                                log!(status_tx, t!("Computing graph boundaries..."));
                                for p in &*file.viewer.persons {
                                    min.x = min.x.min(p.position.x);
                                    min.y = min.y.min(p.position.y);
                                    max.x = max.x.max(p.position.x);
                                    max.y = max.y.max(p.position.y);
                                }
                                let center = (min + max) / 2.0;
                                let mut cam = Camera::new(center);
                                // cam is normalized on the [-1, 1] range
                                // compute x and y scaling to fit the circle, take the best
                                let fig_size = max - min;
                                let scale_x = 1.0 / fig_size.x;
                                let scale_y = 1.0 / fig_size.y;
                                let scale = scale_x.min(scale_y) * 0.98;
                                cam.transf.append_scaling_mut(scale);

                                let tab = tabs::create_tab(
                                    file.viewer,
                                    file.edges.iter(),
                                    gl_fwd,
                                    110,
                                    cam,
                                    UiState::default(),
                                    status_tx,
                                )?;

                                state_tx.send(tab)?;

                                Ok(())
                            });
                        }
                    }
                    AppState::Loaded { tree, .. } => {
                        DockArea::new(tree)
                            .style({
                                let style = Style::from_egui(ctx.style().as_ref());
                                style
                            })
                            .show_leaf_collapse_buttons(false)
                            .show_leaf_close_all_buttons(false)
                            .show_inside(
                                ui,
                                &mut TabViewer {
                                    tab_request: &mut new_tab_request,
                                    top_bar: &mut self.top_bar,
                                    frame,
                                    modal: self.modal.1.clone(),
                                },
                            );
                        if let Some(request) = new_tab_request {
                            tree.push_to_focused_leaf(request);
                        }
                    }
                };

                if !self.top_bar {
                    let rect = ctx.screen_rect().translate(vec2(-4.0, 26.0));
                    if ui
                        .put(rect, |ui: &mut Ui| {
                            ui.with_layout(Layout::default().with_cross_align(Align::RIGHT), |ui| {
                                ui.button(t!("⏬ Show header"))
                            })
                            .inner
                        })
                        .clicked()
                    {
                        self.top_bar = true;
                    }
                }
            });
    }
}

impl GraphViewApp {
    fn show_top_bar(&mut self, ctx: &Context, shown: bool) {
        egui::TopBottomPanel::top("top_panel").show_animated(ctx, shown, |ui| {
            ui.add_space(10.0);
            macro_rules! hide_header {
                ($ui:expr) => {
                    if $ui.button(t!("Hide header ⏫")).clicked() {
                        self.top_bar = false;
                    }
                }
            };
            let small_window = ctx.screen_rect().width() < 1100.0;
            ui.horizontal(|ui| {
                //ui.spacing_mut().item_spacing.x = 50.0;
                ui.vertical(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let commit = env!("VERGEN_GIT_SHA");
                        ui.label("Commit ");
                        ui.add(
                            Hyperlink::from_label_and_url(
                                commit,
                                format!(
                                    "https://github.com/zdimension/graphrust/commit/{}",
                                    commit
                                ),
                            )
                                .open_in_new_tab(true),
                        );
                        ui.label(format!(" ({})", env!("VERGEN_BUILD_DATE")));
                    });
                    ui.add(
                        Hyperlink::from_label_and_url("zdimension", "https://zdimension.fr")
                            .open_in_new_tab(true),
                    );
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        ui.vertical(|ui| {
                            egui::widgets::global_theme_preference_buttons(ui);
                            ui.horizontal(|ui| {
                                let locale = rust_i18n::locale();
                                for (iso, name) in [("en", "English"), ("fr", "Français")] {
                                    if ui.selectable_label(&*locale == iso, name).clicked() {
                                        rust_i18n::set_locale(iso);
                                    }
                                }
                            });
                            if small_window {
                                hide_header!(ui);
                            }
                        });
                        ui.add_space(15.0);
                    });
                });
                ui.separator();
                ui.vertical(|ui| {
                    CommonMarkViewer::new().show(ui, &mut self.md_cache, &t!(
"If the app is **slow**:
- uncheck **Show links**
- increase **Minimum degree**"));
                });
                ui.separator();
                ui.vertical(|ui| {
                    CommonMarkViewer::new().show(ui, &mut self.md_cache, &t!(
"Each **node** in the graph is a **Facebook account**, and two nodes are **linked** if they are **friends**.

A **group** of accounts **strongly connected** to each other forms a **class**, represented by a **color**.

Nodes are positioned so as to group together strongly connected classes."));
                });
                if !small_window {
                    ui.with_layout(Layout::default().with_cross_align(Align::RIGHT), |ui| {
                        ui.with_layout(Layout::bottom_up(Align::RIGHT), |ui| {
                            hide_header!(ui);
                        });
                    });
                }
            });
            ui.add_space(10.0);
        });
    }
}
