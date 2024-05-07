use std::error::Error;
use crate::camera::{Camera, CamXform};
use std::marker::PhantomData;
use std::ops::Deref;

use crate::graph_storage::{load_binary, ProcessedData};
use crate::ui::{DisplaySection, SelectedUserField, UiState};
use eframe::glow::HasContext;
use eframe::{egui_glow, glow};
use egui::{
    pos2, vec2, Color32, Context, Hyperlink, Id, RichText, TextStyle, Ui, Vec2, WidgetText,
};
use egui_dock::{DockArea, DockState, Style};
use graph_format::{Color3f, EdgeStore, Point};
use graphrust_macros::md;
use itertools::Itertools;
use nalgebra::{Isometry3, Matrix4, Similarity3, Translation3, UnitQuaternion, Vector4};
use simsearch::{SearchOptions, SimSearch};

use egui::epaint::TextShape;
use egui::Event::PointerButton;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc, Mutex};

#[cfg(not(target_arch = "wasm32"))]
pub use std::thread;
#[cfg(target_arch = "wasm32")]
pub use wasm_thread as thread;

#[macro_export]
macro_rules! log {
    ($ch:expr, $($arg:tt)*) => {
        {
            let msg = format!($($arg)*);
            log::info!("{}", &msg);
            $ch.send(msg.clone())?;
        }
    }
}

#[macro_export]
macro_rules! log_progress {
    ($ch: expr, $val:expr, $max:expr) => {
        {
            $ch.send($crate::app::Progress {
                max: $max,
                val: $val,
            })?;
        }
    }
}

#[macro_export]
macro_rules! for_progress {
    ($ch:expr, $var:pat in $iter:expr, $block:block) => {
        {
            let max = ExactSizeIterator::len(&$iter);
            let how_often = (max / 100).max(1);
            for (i, $var) in $iter.enumerate() {
                $block;
                if i % how_often == 0 {
                    log_progress!($ch, i, max);
                }
            }
        }
    }
}

#[macro_export]
macro_rules! ignore_error {
    ($e:expr) => {
        {
            let _: Result<_, std::sync::mpsc::SendError<_>> = try { let _ = $e; () };
        }
    }
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
    ) -> Person {
        Person {
            position,
            size,
            modularity_class,
            id,
            name,
            neighbors: Vec::with_capacity(4),
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Vertex {
    pub position: Point,
    pub color: Color3f,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct PersonVertex {
    pub vertex: Vertex,
    pub degree_and_class: u32,
}

impl PersonVertex {
    pub fn new(position: Point, color: Color3f, degree: u16, class: u16) -> PersonVertex {
        PersonVertex {
            vertex: Vertex::new(position, color),
            degree_and_class: ((class as u32) << 16) | (degree as u32),
        }
    }
}
//implement_vertex!(Vertex, position, color);

impl Vertex {
    pub fn new(position: Point, color: Color3f) -> Vertex {
        Vertex { position, color }
    }
}

#[derive(Clone)]
pub struct ModularityClass {
    pub color: Color3f,
    pub id: u16,
    pub name: String,
}

impl ModularityClass {
    pub fn new(color: Color3f, id: u16) -> ModularityClass {
        ModularityClass {
            color,
            id,
            name: format!("Classe {}", id),
        }
    }

    /*pub fn get_people<'a>(&mut self, data: &ViewerData<'a>) -> &Vec<&Person<'a>> {
        data.persons
            .iter()
            .filter(|p| p.modularity_class == self.id)
            .collect()
    }*/
}

pub struct CancelableError;

impl<T: Error> From<T> for CancelableError {
    fn from(_: T) -> Self {
        CancelableError
    }
}

pub type Cancelable<T> = Result<T, CancelableError>;

#[derive(Clone)]
pub struct ViewerData {
    pub persons: Vec<Person>,
    pub modularity_classes: Vec<ModularityClass>,
    pub engine: SimSearch<usize>,
}

impl ViewerData {
    pub fn new(
        persons: Vec<Person>,
        modularity_classes: Vec<ModularityClass>,
        status_tx: &StatusWriter,
    ) -> Cancelable<ViewerData> {
        log!(status_tx, "Initializing search engine");
        let mut engine: SimSearch<usize> =
            SimSearch::new_with(SearchOptions::new().stop_words(vec!["-".into()]));
        for (i, person) in persons.iter().enumerate() {
            engine.insert(i, person.name);
        }
        log!(status_tx, "Done");
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

#[derive(Copy, Clone)]
pub enum CamAnimating {
    Pan(Vec2),
    Rot(f32),
    PanTo { from: CamXform, to: CamXform },
}


pub struct TabCamera {
    pub camera: Camera,
    pub camera_default: Camera,
    pub cam_animating: Option<CamAnimating>,
}

pub struct GraphTabLoaded {
    pub ui_state: UiState,
    pub viewer_data: Arc<ViewerData>,
    pub rendered_graph: Arc<Mutex<RenderedGraph>>,
    pub tab_camera: TabCamera,
}

pub enum GraphTabState {
    Loading {
        status: String,
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
            status: "Chargement du graphe...".to_string(),
            status_rx,
            state_rx,
            gl_mpsc,
        }
    }
}

pub struct GraphTab {
    pub title: String,
    pub closeable: bool,
    pub state: GraphTabState,
}

pub struct StatusWriter {
    tx: Sender<StatusData>,
    ctx: ContextUpdater,
}

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

#[derive(Copy, Clone)]
pub struct Progress {
    pub max: usize,
    pub val: usize,
}

pub struct StatusReader {
    status: String,
    progress: Option<Progress>,
    rx: Receiver<StatusData>,
}

pub enum StatusData {
    Message(String),
    Progress(Progress),
}

impl From<String> for StatusData {
    fn from(s: String) -> Self {
        StatusData::Message(s)
    }
}

impl From<Progress> for StatusData {
    fn from(p: Progress) -> Self {
        StatusData::Progress(p)
    }
}

impl StatusWriter {
    pub fn send(&self, s: impl Into<StatusData>) -> Result<(), mpsc::SendError<StatusData>> {
        if let Err(e) = self.tx.send(s.into()) {
            return Err(e);
        }
        self.ctx.update();
        Ok(())
    }
}

impl StatusReader {
    pub fn recv(&mut self) -> &str {
        if let Ok(s) = self.rx.try_recv() {
            match s {
                StatusData::Message(s) => {
                    self.progress = None;
                    if !self.status.is_empty() {
                        self.status.push('\n');
                    }
                    self.status.push_str(&s);
                }
                StatusData::Progress(p) => {
                    self.progress = Some(p);
                }
            }
        }
        &self.status
    }
}

pub fn status_pipe(ctx: &Context) -> (StatusWriter, StatusReader) {
    let (tx, rx) = mpsc::channel();
    (
        StatusWriter {
            tx,
            ctx: ContextUpdater::new(ctx),
        },
        StatusReader {
            status: "".to_string(),
            progress: None,
            rx,
        },
    )
}

pub enum GlWorkResult {
    Shaders([glow::Program; 3]),
    PathArray(glow::VertexArray, glow::Buffer),
    VertArray(glow::VertexArray, glow::Buffer),
}

trait GlWorkGetter<R>: FnOnce(&glow::Context) -> R {
    fn get(rx: &Receiver<GlWorkResult>) -> R;
    fn to_boxed(self) -> GlWork;
}

impl<T: Send + FnOnce(&glow::Context) -> GlWorkResult + 'static> GlWorkGetter<GlWorkResult> for T {
    fn get(rx: &Receiver<GlWorkResult>) -> GlWorkResult {
        rx.recv().unwrap()
    }

    fn to_boxed(self) -> GlWork {
        GlWork(Box::new(move |gl, tx| {
            tx.send(self(gl)).unwrap();
        }))
    }
}

impl<T: Send + FnOnce(&glow::Context) -> () + 'static> GlWorkGetter<()> for T {
    fn get(_: &Receiver<GlWorkResult>) -> () {}
    fn to_boxed(self) -> GlWork {
        GlWork(Box::new(move |gl, _| {
            self(gl);
        }))
    }
}

pub struct GlWork(Box<dyn Send + FnOnce(&glow::Context, &Sender<GlWorkResult>)>);

type GlMpsc = (Receiver<GlWork>, Sender<GlWorkResult>);

pub struct GlForwarder {
    tx: Sender<GlWork>,
    rx: Receiver<GlWorkResult>,
}

impl GlForwarder {
    pub fn new() -> (GlForwarder, GlMpsc) {
        let (work_tx, work_rx) = mpsc::channel();
        let (res_tx, res_rx) = mpsc::channel();
        (
            Self {
                tx: work_tx,
                rx: res_rx,
            },
            (work_rx, res_tx),
        )
    }

    pub fn run<R, T: GlWorkGetter<R>>(&self, work: T) -> R {
        self.tx.send(work.to_boxed()).unwrap();
        T::get(&self.rx)
    }
}

pub fn create_tab<'a>(
    viewer: ViewerData,
    edges: impl ExactSizeIterator<Item=&'a EdgeStore>,
    gl: GlForwarder,
    default_filter: u16,
    camera: Camera,
    ui_state: UiState,
    status_tx: StatusWriter,
) -> Cancelable<GraphTabLoaded> {
    log!(status_tx, "Creating tab with {} nodes and {} edges", viewer.persons.len(), edges.len());
    log!(status_tx, "Computing maximum degree...");
    let max_degree = viewer
        .persons
        .iter()
        .map(|p| p.neighbors.len())
        .max()
        .unwrap() as u16;
    log!(status_tx, "Max degree: {}", max_degree);
    let hide_edges = if cfg!(target_arch = "wasm32") {
        edges.len() > 300000
    } else {
        false
    };
    Ok(GraphTabLoaded {
        tab_camera: TabCamera { camera, camera_default: camera, cam_animating: None },
        ui_state: UiState {
            display: DisplaySection {
                node_count: viewer.persons.len(),
                g_opac_edges: (400000.0 / edges.len() as f32).min(0.22),
                g_opac_nodes: ((70000.0 / viewer.persons.len() as f32)
                    * if hide_edges { 5.0 } else { 2.0 })
                    .min(0.58),
                max_degree,
                g_show_edges: !hide_edges,
                ..DisplaySection::default()
            },
            ..ui_state
        },
        rendered_graph: Arc::new(Mutex::new(RenderedGraph {
            degree_filter: (default_filter, u16::MAX),
            ..RenderedGraph::new(gl, &viewer, edges, status_tx)?
        })),
        viewer_data: Arc::from(viewer),
    })
}

pub struct GraphViewApp {
    top_bar: bool,
    state: AppState,
}

pub enum AppState {
    Loading {
        status_rx: StatusReader,
        file_rx: Receiver<ProcessedData>,
    },
    Loaded {
        tree: DockState<GraphTab>,
        #[allow(dead_code)]
        // we do a little trolling
        string_tables: StringTables,
    },
}

impl GraphViewApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let gl = cc
            .gl
            .as_ref()
            .expect("You need to run eframe with the glow backend");
        unsafe {
            gl.enable(glow::PROGRAM_POINT_SIZE);
        }

        let (status_tx, status_rx) = status_pipe(&cc.egui_ctx);
        let (file_tx, file_rx) = mpsc::channel();

        spawn_cancelable(move || {
            file_tx.send(load_binary(status_tx)?)?;
            Ok(())
        });

        return Self {
            top_bar: true,
            state: AppState::Loading { status_rx, file_rx },
        };
    }
}

fn show_status(ui: &mut Ui, status_rx: &mut StatusReader) {
    ui.vertical_centered(|ui| {
        ui.spinner();
        ui.label(status_rx.recv());
        if let Some(p) = status_rx.progress {
            ui.add(egui::ProgressBar::new(p.val as f32 / p.max as f32).desired_height(12.0).desired_width(230.0));
        }
    });
}

pub fn spawn_cancelable(f: impl FnOnce() -> Cancelable<()> + Send + 'static) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        if let Err(_) = f() {
            log::info!("Tab closed; cancelled");
        };
    })
}

struct TabViewer<'graph, 'ctx, 'tab_request, 'frame> {
    ctx: &'ctx egui::Context,
    data: PhantomData<&'graph bool>,
    tab_request: &'tab_request mut Option<NewTabRequest>,
    top_bar: &'tab_request mut bool,
    frame: &'frame mut eframe::Frame,
}

impl<'graph, 'ctx, 'tab_request, 'frame> egui_dock::TabViewer
for TabViewer<'graph, 'ctx, 'tab_request, 'frame>
{
    type Tab = GraphTab;

    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        RichText::from(&tab.title).into()
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        let ctx = self.ctx;

        match &mut tab.state {
            GraphTabState::Loading {
                status,
                status_rx,
                state_rx,
                gl_mpsc,
            } => {
                for work in gl_mpsc.0.try_iter() {
                    work.0(self.frame.gl().unwrap().deref(), &gl_mpsc.1);
                }
                show_status(ui, status_rx);
                if let Ok(state) = state_rx.try_recv() {
                    tab.state = GraphTabState::Loaded(state);
                    ctx.request_repaint();
                }
            }
            GraphTabState::Loaded(tab) => {
                let cid = Id::from("camera").with(ui.id());

                ui.spacing_mut().scroll.floating_allocated_width = 18.0;
                egui::SidePanel::left("settings")
                    .resizable(false)
                    .show_inside(ui, |ui| {
                        if !*self.top_bar {
                            if ui.button("Afficher l'en-tête").clicked() {
                                *self.top_bar = true;
                            }
                        }
                        tab.ui_state.draw_ui(
                            ui,
                            &tab.viewer_data,
                            &mut *tab.rendered_graph.lock().unwrap(),
                            self.tab_request,
                            self.frame,
                            &mut tab.tab_camera,
                            cid,
                        );
                    });
                egui::CentralPanel::default()
                    .frame(egui::Frame {
                        fill: Color32::from_rgba_unmultiplied(255, 255, 255, 0),
                        ..Default::default()
                    })
                    .show_inside(ui, |ui| {
                        let (id, rect) = ui.allocate_space(ui.available_size());

                        let sz = rect.size();
                        if sz != tab.tab_camera.camera.size {
                            tab.tab_camera.camera.set_window_size(sz);
                            tab.tab_camera.camera_default.set_window_size(sz);
                        }

                        let response =
                            ui.interact(rect, id, egui::Sense::click().union(egui::Sense::drag()));

                        if !response.is_pointer_button_down_on() {
                            if let Some(v) = tab.tab_camera.cam_animating {
                                const DUR: f32 = 0.5;
                                let anim = ctx.animate_bool_with_time(cid, false, DUR);
                                if anim == 0.0 {
                                    tab.tab_camera.cam_animating = None;
                                    match v {
                                        CamAnimating::PanTo { to, .. } => {
                                            tab.tab_camera.camera.transf = to;
                                        }
                                        _ => {}
                                    }
                                } else {
                                    match v {
                                        CamAnimating::Pan(delta) => {
                                            tab.tab_camera.camera.pan(delta.x * anim, delta.y * anim);
                                        }
                                        CamAnimating::Rot(rot) => {
                                            tab.tab_camera.camera.rotate(rot * anim);
                                        }
                                        CamAnimating::PanTo { from, to } => {
                                            // egui gives us a value going from 1 to 0, so we flip it.
                                            let t = 1.0 - anim;

                                            /// Maps a linear value to a smooth blend curve (both [0, 1]).
                                            fn blend(x: f32) -> f32 {
                                                let sqr = x * x;
                                                sqr / (2.0 * (sqr - x) + 1.0)
                                            }

                                            let t = blend(t);

                                            /// Linearly interpolates between two values.
                                            fn lerp(from: f32, to: f32, t: f32) -> f32 {
                                                from * (1.0 - t) + to * t
                                            }

                                            tab.tab_camera.camera.transf = Similarity3::from_isometry(
                                                from.isometry.lerp_slerp(&to.isometry, t),
                                                lerp(from.scaling(), to.scaling(), t),
                                            );
                                        }
                                    }
                                }
                            }
                        }

                        if let Some(pos) = response.interact_pointer_pos().or(response.hover_pos())
                        {
                            let centered_pos_raw = pos - rect.center();
                            let centered_pos = 2.0 * centered_pos_raw / rect.size();

                            if response.dragged_by(egui::PointerButton::Primary) {
                                tab.tab_camera.camera
                                    .pan(response.drag_delta().x, response.drag_delta().y);

                                ctx.animate_bool_with_time(cid, true, 0.0);
                                tab.tab_camera.cam_animating = Some(CamAnimating::Pan(response.drag_delta()));
                            } else if response.dragged_by(egui::PointerButton::Secondary) {
                                let prev_pos = centered_pos_raw - response.drag_delta();
                                let rot = centered_pos_raw.angle() - prev_pos.angle();
                                tab.tab_camera.camera.rotate(rot);

                                ctx.animate_bool_with_time(cid, true, 0.0);
                                tab.tab_camera.cam_animating = Some(CamAnimating::Rot(rot));
                            }

                            let zero_pos = (pos - rect.min).to_pos2();

                            tab.ui_state.details.mouse_pos = Some(centered_pos.to_pos2());
                            let pos_world = (tab.tab_camera.camera.get_inverse_matrix()
                                * Vector4::new(centered_pos.x, -centered_pos.y, 0.0, 1.0))
                                .xy();
                            tab.ui_state.details.mouse_pos_world = Some(pos_world);

                            if response.clicked() {
                                let closest = tab
                                    .viewer_data
                                    .persons
                                    .iter()
                                    .map(|p| {
                                        let diff = p.position - pos_world.into();

                                        diff.norm_squared()
                                    })
                                    .enumerate()
                                    .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                                    .map(|(i, _)| i);
                                if let Some(closest) = closest {
                                    log::info!(
                                        "Selected person {}: {:?} (mouse: {:?})",
                                        closest,
                                        tab.viewer_data.persons[closest].position,
                                        pos_world
                                    );
                                    tab.ui_state.infos.infos_current = Some(closest);
                                    tab.ui_state.infos.infos_open = true;

                                    match tab.ui_state.selected_user_field {
                                        SelectedUserField::Selected => {
                                            tab.ui_state.infos.infos_current = Some(closest);
                                            tab.ui_state.infos.infos_open = true;
                                        }
                                        SelectedUserField::PathSource => {
                                            tab.ui_state.path.path_settings.path_src = Some(closest);
                                            tab.ui_state.path.path_dirty = true;
                                            tab.ui_state.selected_user_field =
                                                SelectedUserField::PathDest;
                                        }
                                        SelectedUserField::PathDest => {
                                            tab.ui_state.path.path_settings.path_dest = Some(closest);
                                            tab.ui_state.path.path_dirty = true;
                                        }
                                    }
                                }
                            }

                            let (scroll_delta, zoom_delta, multi_touch) = ui.input(|is| {
                                (is.raw_scroll_delta, is.zoom_delta(), is.multi_touch())
                            });

                            if scroll_delta.y != 0.0 {
                                let zoom_speed = 1.1;
                                let s = if scroll_delta.y > 0.0 {
                                    zoom_speed
                                } else {
                                    1.0 / zoom_speed
                                };
                                tab.tab_camera.camera.zoom(s, zero_pos);
                            }
                            if zoom_delta != 1.0 {
                                tab.tab_camera.camera.zoom(zoom_delta, zero_pos);
                            }

                            if let Some(multi_touch) = multi_touch {
                                tab.tab_camera.camera.rotate(multi_touch.rotation_delta);
                            }
                        } else {
                            tab.ui_state.details.mouse_pos = None;
                            tab.ui_state.details.mouse_pos_world = None;
                        }

                        let graph = tab.rendered_graph.clone();
                        let edges = tab.ui_state.display.g_show_edges;
                        let nodes = tab.ui_state.display.g_show_nodes;
                        let opac_edges = tab.ui_state.display.g_opac_edges;
                        let opac_nodes = tab.ui_state.display.g_opac_nodes;

                        let cam = tab.tab_camera.camera.get_matrix();
                        let callback = egui::PaintCallback {
                            rect,
                            callback: Arc::new(egui_glow::CallbackFn::new(
                                move |_info, painter| {
                                    graph.lock().unwrap().paint(
                                        painter.gl(),
                                        cam,
                                        (edges, opac_edges),
                                        (nodes, opac_nodes),
                                    );
                                },
                            )),
                        };
                        ui.painter().add(callback);

                        let draw_person = |id, color| {
                            let person: &Person = &tab.viewer_data.persons[id];
                            let pos = person.position;
                            let pos_scr = (cam * Vector4::new(pos.x, pos.y, 0.0, 1.0)).xy();
                            let txt = WidgetText::from(person.name)
                                .background_color(color)
                                .color(Color32::WHITE);
                            let gal =
                                txt.into_galley(ui, Some(false), f32::INFINITY, TextStyle::Heading);
                            ui.painter().add(TextShape::new(
                                rect.center()
                                    + vec2(pos_scr.x, -pos_scr.y) * rect.size() * 0.5
                                    + vec2(10.0, 10.0),
                                gal,
                                Color32::TRANSPARENT,
                            ));
                        };

                        if let Some(ref path) = tab.ui_state.path.found_path {
                            for &p in path {
                                draw_person(p, Color32::from_rgba_unmultiplied(150, 0, 0, 200));
                            }
                        }

                        if let Some(sel) = tab.ui_state.infos.infos_current {
                            draw_person(sel, Color32::from_rgba_unmultiplied(0, 100, 0, 200));
                        }
                    });
            }
        }
    }

    fn closeable(&mut self, tab: &mut Self::Tab) -> bool {
        tab.closeable
    }

    fn on_close(&mut self, tab: &mut Self::Tab) -> bool {
        if let GraphTabState::Loaded(tab) = &tab.state {
            tab.rendered_graph
                .lock()
                .unwrap()
                .destroy(&self.frame.gl().unwrap().clone());
        }
        true
    }
}

pub type NewTabRequest = GraphTab;

impl eframe::App for GraphViewApp {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let mut new_tab_request = None;

        if self.top_bar {
            self.show_top_bar(ctx);
        }

        match &mut self.state {
            AppState::Loading { status_rx, file_rx } => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    show_status(ui, status_rx);
                });
                if let Ok(file) = file_rx.try_recv() {
                    let (status_tx, status_rx) = status_pipe(ctx);
                    let (state_tx, state_rx) = mpsc::channel();
                    let (gl_fwd, gl_mpsc) = GlForwarder::new();
                    self.state = AppState::Loaded {
                        tree: DockState::new(vec![GraphTab {
                            closeable: false,
                            title: "Graphe".to_string(),
                            state: GraphTabState::loading(status_rx, state_rx, gl_mpsc),
                        }]),
                        string_tables: file.strings,
                    };
                    spawn_cancelable(move || {
                        let mut min = Point::new(f32::INFINITY, f32::INFINITY);
                        let mut max = Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
                        log!(status_tx, "Calcul des limites du graphes...");
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

                        let tab = create_tab(
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
            AppState::Loaded {
                tree,
                string_tables,
            } => {
                DockArea::new(tree)
                    .style({
                        let style = Style::from_egui(ctx.style().as_ref());
                        style
                    })
                    .show(
                        ctx,
                        &mut TabViewer {
                            ctx,
                            data: PhantomData,
                            tab_request: &mut new_tab_request,
                            top_bar: &mut self.top_bar,
                            frame,
                        },
                    );
                if let Some(request) = new_tab_request {
                    tree.push_to_focused_leaf(request);
                }
            }
        };
    }
}

impl GraphViewApp {
    fn show_top_bar(&mut self, ctx: &Context) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 50.0;
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
                    egui::widgets::global_dark_light_mode_switch(ui);
                    if ui.button("Réduire").clicked() {
                        self.top_bar = false;
                    }
                });
                ui.vertical(|ui| {
                    md!(ui, r#"
Si l'interface est **lente**:
- décocher **Afficher les liens**
- augmenter **Degré minimum**
                    "#);
                });
                ui.vertical(|ui| {
                    md!(ui, r#"
Chaque **nœud** du graphe est un **compte Facebook**, et deux nœuds sont **reliés** s'ils sont **amis**.

Un **groupe** de comptes **fortement connectés** entre eux forme une **classe**, représentée par une **couleur**.

Les nœuds sont positionnés de sorte à regrouper ensemble les classes fortement connectées."#);
                });
            });
            ui.add_space(10.0);
        });
    }
}

pub struct RenderedGraph {
    pub program_node: glow::Program,
    pub program_basic: glow::Program,
    pub program_edge: glow::Program,
    pub nodes_buffer: glow::Buffer,
    pub nodes_count: usize,
    pub nodes_array: glow::VertexArray,
    pub edges_count: usize,
    pub path_array: glow::VertexArray,
    pub path_buffer: glow::Buffer,
    pub path_count: usize,
    pub new_path: Option<Vec<Vertex>>,
    pub degree_filter: (u16, u16),
    pub filter_nodes: bool,
    pub destroyed: bool,
}

impl RenderedGraph {
    pub fn new<'a>(
        gl: GlForwarder,
        viewer: &ViewerData,
        edges: impl ExactSizeIterator<Item=&'a EdgeStore>,
        status_tx: StatusWriter,
    ) -> Cancelable<Self> {
        use glow::HasContext as _;
        use GlWorkResult::*;

        let shader_version = if cfg!(target_arch = "wasm32") {
            "#version 300 es"
        } else {
            "#version 330"
        };

        unsafe {
            let programs = [
                [
                    (glow::VERTEX_SHADER, include_str!("shaders/basic.vert")),
                    (glow::FRAGMENT_SHADER, include_str!("shaders/basic.frag")),
                ],
                [
                    (glow::VERTEX_SHADER, include_str!("shaders/graph.vert")),
                    (
                        glow::FRAGMENT_SHADER,
                        include_str!("shaders/graph_edge.frag"),
                    ),
                ],
                [
                    (glow::VERTEX_SHADER, include_str!("shaders/graph.vert")),
                    (
                        glow::FRAGMENT_SHADER,
                        include_str!("shaders/graph_node.frag"),
                    ),
                ],
            ];

            log!(status_tx, "Compiling shaders");
            let Shaders([program_basic, program_edge, program_node]) = gl.run(move |gl| {
                Shaders(programs.map(|shader_sources| {
                    let program = gl.create_program().expect("Cannot create program");

                    let shaders: Vec<_> = shader_sources
                        .iter()
                        .map(|(shader_type, shader_source)| {
                            let shader = gl
                                .create_shader(*shader_type)
                                .expect("Cannot create shader");
                            gl.shader_source(shader, &format!("{shader_version}\n{shader_source}"));
                            gl.compile_shader(shader);
                            assert!(
                                gl.get_shader_compile_status(shader),
                                "Failed to compile {shader_type}: {}",
                                gl.get_shader_info_log(shader)
                            );
                            gl.attach_shader(program, shader);
                            shader
                        })
                        .collect();

                    gl.link_program(program);
                    assert!(
                        gl.get_program_link_status(program),
                        "{}",
                        gl.get_program_info_log(program)
                    );

                    for shader in shaders {
                        gl.detach_shader(program, shader);
                        gl.delete_shader(shader);
                    }

                    program
                }))
            }) else {
                panic!("Failed to compile shaders");
            };

            #[cfg(target_arch = "wasm32")]
                let edges = edges.take(10_000_000);

            let edges_count = edges.len();
            log!(status_tx, "Creating vertice list");
            let node_vertices = viewer
                .persons
                .iter()
                .map(|p| {
                    PersonVertex::new(
                        p.position,
                        viewer.modularity_classes[p.modularity_class as usize].color,
                        p.neighbors.len() as u16,
                        p.modularity_class,
                    )
                });
            let edge_vertices = edges
                .map(|e| {
                    let pa = &viewer.persons[e.a as usize];
                    let pb = &viewer.persons[e.b as usize];
                    (pa, pb)
                })
                //.filter(|(pa, pb)| pa.neighbors.len() > 5 && pb.neighbors.len() > 5)
                .flat_map(|(pa, pb)| {
                    let a = pa.position;
                    let b = pb.position;
                    const EDGE_HALF_WIDTH: f32 = 0.75;
                    let ortho = (b - a).ortho().normalized() * EDGE_HALF_WIDTH;
                    let v0 = a + ortho;
                    let v1 = a - ortho;
                    let v2 = b - ortho;
                    let v3 = b + ortho;
                    let color_a =
                        viewer.modularity_classes[pa.modularity_class as usize].color;
                    let color_b =
                        viewer.modularity_classes[pb.modularity_class as usize].color;
                    [
                        PersonVertex::new(
                            v0,
                            color_a,
                            pa.neighbors.len() as u16,
                            pa.modularity_class,
                        ),
                        PersonVertex::new(
                            v1,
                            color_a,
                            pa.neighbors.len() as u16,
                            pa.modularity_class,
                        ),
                        PersonVertex::new(
                            v2,
                            color_b,
                            pb.neighbors.len() as u16,
                            pb.modularity_class,
                        ),
                        PersonVertex::new(
                            v2,
                            color_b,
                            pb.neighbors.len() as u16,
                            pb.modularity_class,
                        ),
                        PersonVertex::new(
                            v3,
                            color_b,
                            pb.neighbors.len() as u16,
                            pb.modularity_class,
                        ),
                        PersonVertex::new(
                            v0,
                            color_a,
                            pa.neighbors.len() as u16,
                            pa.modularity_class,
                        ),
                    ]
                });
            let vertices = node_vertices
                .chain(edge_vertices)
                .collect_vec();

            log!(status_tx, "Buffering {} vertices", vertices.len());
            let VertArray(vertices_array, vertices_buffer) = gl.run(move |gl: &glow::Context| {
                let vertices_array = gl
                    .create_vertex_array()
                    .expect("Cannot create vertex array");
                gl.bind_vertex_array(Some(vertices_array));
                let vertices_buffer = gl.create_buffer().expect("Cannot create buffer");
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertices_buffer));
                gl.buffer_data_u8_slice(
                    glow::ARRAY_BUFFER,
                    std::slice::from_raw_parts(
                        vertices.as_ptr() as *const u8,
                        vertices.len() * std::mem::size_of::<PersonVertex>(),
                    ),
                    glow::STATIC_DRAW,
                );

                gl.vertex_attrib_pointer_f32(
                    0,
                    2,
                    glow::FLOAT,
                    false,
                    std::mem::size_of::<PersonVertex>() as i32,
                    0,
                );
                gl.enable_vertex_attrib_array(0);
                gl.vertex_attrib_pointer_f32(
                    1,
                    3,
                    glow::FLOAT,
                    false,
                    std::mem::size_of::<PersonVertex>() as i32,
                    std::mem::size_of::<Point>() as i32,
                );
                gl.enable_vertex_attrib_array(1);
                gl.vertex_attrib_pointer_i32(
                    2,
                    1,
                    glow::UNSIGNED_INT,
                    std::mem::size_of::<PersonVertex>() as i32,
                    std::mem::size_of::<Vertex>() as i32,
                );
                gl.enable_vertex_attrib_array(2);

                VertArray(vertices_array, vertices_buffer)
            }) else {
                panic!("Failed to create vertices array");
            };

            log!(status_tx, "Creating path array");
            let PathArray(path_array, path_buffer) = gl.run(|gl| {
                let path_array = gl
                    .create_vertex_array()
                    .expect("Cannot create vertex array");
                let path_buffer = gl.create_buffer().expect("Cannot create buffer");

                gl.bind_vertex_array(Some(path_array));
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(path_buffer));
                gl.vertex_attrib_pointer_f32(
                    0,
                    2,
                    glow::FLOAT,
                    false,
                    std::mem::size_of::<Vertex>() as i32,
                    0,
                );
                gl.enable_vertex_attrib_array(0);
                gl.vertex_attrib_pointer_f32(
                    1,
                    3,
                    glow::FLOAT,
                    false,
                    std::mem::size_of::<Vertex>() as i32,
                    std::mem::size_of::<Point>() as i32,
                );
                gl.enable_vertex_attrib_array(1);

                PathArray(path_array, path_buffer)
            }) else {
                panic!("Failed to create path array");
            };

            log!(status_tx, "Done: {}", chrono::Local::now().format("%H:%M:%S.%3f"));

            Ok(Self {
                program_basic,
                program_edge,
                program_node,
                nodes_buffer: vertices_buffer,
                nodes_count: viewer.persons.len(),
                nodes_array: vertices_array,
                edges_count,
                path_array,
                path_buffer,
                path_count: 0,
                new_path: None,
                degree_filter: (0, u16::MAX),
                filter_nodes: false,
                destroyed: false,
            })
        }
    }

    fn destroy(&mut self, gl: &glow::Context) {
        log::info!("Destroying graph");
        self.destroyed = true;
        use glow::HasContext as _;
        unsafe {
            log::info!("Deleting programs");
            gl.delete_program(self.program_basic);
            gl.delete_program(self.program_edge);
            gl.delete_program(self.program_node);
            log::info!("Deleting buffers");
            gl.delete_buffer(self.nodes_buffer);
            gl.delete_buffer(self.path_buffer);
            log::info!("Deleting arrays");
            gl.delete_vertex_array(self.nodes_array);
            gl.delete_vertex_array(self.path_array);
        }
    }

    fn paint(
        &mut self,
        gl: &glow::Context,
        cam: Matrix4<f32>,
        edges: (bool, f32),
        nodes: (bool, f32),
    ) {
        if self.destroyed {
            return;
        }

        use glow::HasContext as _;
        unsafe {
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

            gl.bind_vertex_array(Some(self.nodes_array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.nodes_buffer));
            if edges.0 {
                gl.use_program(Some(self.program_edge));
                gl.uniform_matrix_4_f32_slice(
                    Some(
                        &gl.get_uniform_location(self.program_edge, "u_projection")
                            .unwrap(),
                    ),
                    false,
                    &cam.as_slice(),
                );
                gl.uniform_1_u32(
                    Some(
                        &gl.get_uniform_location(self.program_edge, "u_degfilter")
                            .unwrap(),
                    ),
                    ((self.degree_filter.1 as u32) << 16) | (self.degree_filter.0 as u32),
                );
                gl.uniform_1_f32(
                    Some(
                        &gl.get_uniform_location(self.program_edge, "opacity")
                            .unwrap(),
                    ),
                    edges.1,
                );
                let verts = 2 * 3 * self.edges_count as i32;
                // if wasm, clamp verts at 30M, because Firefox refuses to draw anything above that
                #[cfg(target_arch = "wasm32")]
                    let verts = verts.min(30_000_000);
                gl.draw_arrays(glow::TRIANGLES, self.nodes_count as i32, verts);
            }
            if nodes.0 {
                gl.use_program(Some(self.program_node));
                gl.uniform_matrix_4_f32_slice(
                    Some(
                        &gl.get_uniform_location(self.program_node, "u_projection")
                            .unwrap(),
                    ),
                    false,
                    &cam.as_slice(),
                );
                gl.uniform_1_u32(
                    Some(
                        &gl.get_uniform_location(self.program_node, "u_degfilter")
                            .unwrap(),
                    ),
                    if self.filter_nodes {
                        ((self.degree_filter.1 as u32) << 16) | (self.degree_filter.0 as u32)
                    } else {
                        0xffff_0000
                    },
                );
                gl.uniform_1_f32(
                    Some(
                        &gl.get_uniform_location(self.program_node, "opacity")
                            .unwrap(),
                    ),
                    nodes.1,
                );
                gl.draw_arrays(glow::POINTS, 0, self.nodes_count as i32);
            }

            gl.use_program(Some(self.program_basic));
            gl.uniform_matrix_4_f32_slice(
                Some(
                    &gl.get_uniform_location(self.program_basic, "u_projection")
                        .unwrap(),
                ),
                false,
                &cam.as_slice(),
            );
            gl.bind_vertex_array(Some(self.path_array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.path_buffer));
            if let Some(path) = self.new_path.take() {
                log::info!("Buffering {} path vertices", path.len());
                if path.is_empty() {
                    self.path_count = 0;
                } else {
                    gl.buffer_data_u8_slice(
                        glow::ARRAY_BUFFER,
                        std::slice::from_raw_parts(
                            path.as_ptr() as *const u8,
                            path.len() * std::mem::size_of::<Vertex>(),
                        ),
                        glow::STATIC_DRAW,
                    );
                    self.path_count = path.len();
                }
            }

            if self.path_count > 0 {
                gl.draw_arrays(glow::TRIANGLES, 0, self.path_count as i32);
            }
        }
    }
}
