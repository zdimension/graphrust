use crate::camera::{CamXform, Camera};
use std::collections::VecDeque;
use std::error::Error;
use std::fmt::Display;
use std::ops::Deref;

use crate::graph_storage::{load_binary, load_file, ProcessedData};
use crate::ui::{DisplaySection, PathStatus, SelectedUserField, UiState};
use eframe::glow::HasContext;
use eframe::{egui_glow, glow};
use egui::{vec2, Color32, Context, Hyperlink, Id, RichText, Stroke, TextStyle, Ui, Vec2, WidgetText};
use egui_dock::{DockArea, DockState, Style};
use graph_format::nalgebra::{Isometry3, Matrix4, Similarity3, Translation3, UnitQuaternion, Vector4};
use graph_format::{Color3b, Color3f, EdgeStore, Point};
use graphrust_macros::md;
use itertools::Itertools;

use egui::epaint::{CircleShape, PathStroke, TextShape};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc};
use zearch::Index;

use eframe::epaint::text::TextWrapMode;
use egui::Shape::LineSegment;
use egui_modal::{Icon, Modal};
use parking_lot::lock_api::{RwLockReadGuard, RwLockWriteGuard};
use parking_lot::{RawRwLock, RwLock};
#[cfg(not(target_arch = "wasm32"))]
pub use std::thread;
#[cfg(target_arch = "wasm32")]
pub use wasm_thread as thread;

#[macro_export]
macro_rules! log {
    ($ch:expr, $($arg:tt)*) => {
        {
            use $crate::app::StatusWriterInterface;
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
            use $crate::app::StatusWriterInterface;
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
            for (i_, $var) in $iter.enumerate() {
                $block;
                if i_ % how_often == 0 {
                    log_progress!($ch, i_, max);
                }
            }
        }
    }
}

#[macro_export]
macro_rules! ignore_error {
    ($e:expr) => {
        {
            let _: Result<_, std::sync::mpsc::SendError<_>> = try { let _ = $e; };
        }
    }
}

pub fn iter_progress<'a, T>(iter: T, ch: &'a StatusWriter) -> impl Iterator<Item=T::Item> + 'a
where
    T: ExactSizeIterator + 'a,
{
    let max = ExactSizeIterator::len(&iter);
    let how_often = (max / 100).max(1);
    iter.enumerate().map(move |(i, x)| {
        if i % how_often == 0 {
            ignore_error!(log_progress!(ch, i, max));
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

impl AsRef<str> for Person {
    fn as_ref(&self) -> &str {
        self.name
    }
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

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Vertex {
    pub position: Point,
    pub color: Color3b,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct PersonVertex {
    pub position: Point,
    pub degree_and_class: u32,
}

impl PersonVertex {
    pub fn new(position: Point, degree: u16, class: u16) -> PersonVertex {
        PersonVertex {
            position,
            degree_and_class: ((class as u32) << 16) | (degree as u32),
        }
    }
}
//implement_vertex!(Vertex, position, color);

impl Vertex {
    pub fn new(position: Point, color: Color3b) -> Vertex {
        Vertex { position, color }
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

    /*pub fn get_people<'a>(&mut self, data: &ViewerData<'a>) -> &Vec<&Person<'a>> {
        data.persons
            .iter()
            .filter(|p| p.modularity_class == self.id)
            .collect()
    }*/
}

#[derive(Debug)]
pub enum CancelableError {
    TabClosed,
    Other(anyhow::Error),
}

impl Display for CancelableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CancelableError::TabClosed => write!(f, "Tab closed"),
            CancelableError::Other(e) => write!(f, "Other error: {}", e),
        }
    }
}

impl<T: Error> From<T> for CancelableError {
    default fn from(_: T) -> CancelableError {
        CancelableError::TabClosed
    }
}

impl<T: Error + Into<anyhow::Error>> From<T> for CancelableError {
    fn from(e: T) -> Self {
        CancelableError::Other(e.into())
    }
}

pub type Cancelable<T> = Result<T, CancelableError>;

#[derive(Clone)]
pub struct ViewerData {
    pub persons: Vec<Person>,
    pub modularity_classes: Vec<ModularityClass>,
    pub engine: Index<'static>,
}

impl ViewerData {
    pub fn new(
        persons: Vec<Person>,
        modularity_classes: Vec<ModularityClass>,
        status_tx: &impl StatusWriterInterface,
    ) -> Cancelable<ViewerData> {
        log!(status_tx, "Initializing search engine");
        let engine = Index::new_in_memory(unsafe { std::mem::transmute::<&[Person], &'static [Person]>(&persons[..]) });
        log!(status_tx, "Done");
        Ok(ViewerData {
            persons,
            modularity_classes,
            engine,
        })
    }

    pub fn get_edges(&self) -> impl Iterator<Item=(usize, usize)> + '_ {
        self
            .persons
            .iter()
            .enumerate()
            .flat_map(|(i, n)| {
                n.neighbors.iter()
                    .filter(move |&&j| i < j)
                    .map(move |&j| (i, j))
            })
    }

    pub fn replace_data(
        &self,
        persons: Vec<Person>,
        modularity_classes: Vec<ModularityClass>,
    ) -> ViewerData {
        ViewerData {
            persons,
            modularity_classes,
            engine: self.engine.clone(),
        }
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

#[derive(Default)]
pub struct MyRwLock<T> {
    inner: RwLock<T>,
}

impl<T> MyRwLock<T> {
    pub fn new(x: T) -> MyRwLock<T> {
        MyRwLock { inner: RwLock::new(x) }
    }

    pub fn read(&self) -> RwLockReadGuard<'_, RawRwLock, T> {
        #[cfg(target_arch = "wasm32")]
        {
            // Using an RwLock for the main graph state is the most logical choice, but it means
            // the main thread can technically block a bit if there's a background thread doing
            // write work. This is okay on desktop, but ends up being a problem on wasm since we
            // can't block the main thread (this is enforced by preventing the use of Atomics.wait
            // on the main thread). However, we (the developer) know that even if the main thread
            // does get blocked, it won't be for more than a couple of milliseconds. So we do a
            // little active wait. This will never, ever end up coming back to bite us. Ever.
            loop {
                if let Some(lock) = self.inner.try_read() {
                    return lock;
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.inner.read()
        }
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, RawRwLock, T> {
        #[cfg(target_arch = "wasm32")]
        {
            let start = chrono::Utc::now();
            loop {
                if let Some(lock) = self.inner.try_write() {
                    return lock;
                }
                if chrono::Utc::now() - start > chrono::Duration::milliseconds(100) {
                    panic!("Locking took too long");
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.inner.write()
        }
    }
}

pub struct TabCamera {
    pub camera: Camera,
    pub camera_default: Camera,
    pub cam_animating: Option<CamAnimating>,
}

pub struct GraphTabLoaded {
    pub ui_state: UiState,
    pub viewer_data: Arc<MyRwLock<ViewerData>>,
    pub rendered_graph: Arc<MyRwLock<RenderedGraph>>,
    pub tab_camera: TabCamera,
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

pub struct GraphTab {
    pub id: Id,
    pub title: String,
    pub closeable: bool,
    pub state: GraphTabState,
}

#[derive(Clone)]
pub struct StatusWriter {
    tx: Sender<StatusData>,
    ctx: ContextUpdater,
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

pub trait StatusWriterInterface {
    fn send(&self, s: impl Into<StatusData>) -> Result<(), mpsc::SendError<StatusData>>;
}

pub struct NullStatusWriter;

impl StatusWriterInterface for NullStatusWriter {
    fn send(&self, _: impl Into<StatusData>) -> Result<(), mpsc::SendError<StatusData>> {
        Ok(())
    }
}

impl StatusWriterInterface for StatusWriter {
    fn send(&self, s: impl Into<StatusData>) -> Result<(), mpsc::SendError<StatusData>> {
        self.tx.send(s.into())?;
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

pub trait GlWorkGetter<R>: FnOnce(&glow::Context) -> R {
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

impl<T: Send + FnOnce(&glow::Context) + 'static> GlWorkGetter<()> for T {
    fn get(_: &Receiver<GlWorkResult>) {}
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
        rendered_graph: Arc::new(MyRwLock::new(RenderedGraph {
            degree_filter: (default_filter, u16::MAX),
            ..RenderedGraph::new(gl, &viewer, edges, status_tx)?
        })),
        viewer_data: Arc::from(MyRwLock::new(viewer)),
    })
}

pub struct GraphViewApp {
    top_bar: bool,
    modal: (Receiver<ModalInfo>, Sender<ModalInfo>),
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
        /// we do a little trolling
        ///
        /// this is for keeping the StringTables object allocated since the graph objects have
        /// `&'static str`s pointing to it
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
        let (modal_tx, modal_rx) = mpsc::channel();

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
        spawn_cancelable(modal_tx.clone(), move || {
            let res = load_file(&status_tx)?;
            let res = load_binary(&status_tx, res)?;
            file_tx.send(res)?;
            Ok(())
        });

        Self {
            top_bar: true,
            modal: (modal_rx, modal_tx),
            state: AppState::Loading { status_rx, file_rx },
        }
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

pub struct ModalInfo {
    title: String,
    body: String,
}

pub trait ModalWriter: Clone + Send + 'static {
    fn send(&self, modal: ModalInfo);
}

impl ModalWriter for Sender<ModalInfo> {
    fn send(&self, modal: ModalInfo) {
        if let Err(e) = self.send(modal) {
            log::error!("Error sending modal: {}", e);
        }
    }
}

pub fn spawn_cancelable(ms: impl ModalWriter, f: impl FnOnce() -> Cancelable<()> + Send + 'static) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        match f() {
            Err(CancelableError::TabClosed) => {
                log::info!("Tab closed; cancelled");
            }
            Err(CancelableError::Other(e)) => {
                ms.send(ModalInfo {
                    title: "Error".to_string(),
                    body: format!("Error: {}", e),
                });
            }
            Ok(()) => {}
        }
    })
}

struct TabViewer<'tab_request, 'frame> {
    tab_request: &'tab_request mut Option<NewTabRequest>,
    top_bar: &'tab_request mut bool,
    frame: &'frame mut eframe::Frame,
    modal: Sender<ModalInfo>,
}

impl egui_dock::TabViewer for TabViewer<'_, '_>
{
    type Tab = GraphTab;

    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        RichText::from(&tab.title).into()
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        match &mut tab.state {
            GraphTabState::Loading {
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
                    ui.ctx().request_repaint();
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
                            &tab.rendered_graph,
                            self.tab_request,
                            &mut tab.tab_camera,
                            cid,
                            &self.modal,
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
                                let anim = ui.ctx().animate_bool_with_time(cid, false, DUR);
                                if anim == 0.0 {
                                    tab.tab_camera.cam_animating = None;
                                    match v {
                                        CamAnimating::PanTo { to, .. } => {
                                            tab.tab_camera.camera.transf = to;
                                        }
                                        _ => {
                                            // only PanTo is animated and needs to pin the final value
                                        }
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

                                ui.ctx().animate_bool_with_time(cid, true, 0.0);
                                tab.tab_camera.cam_animating = Some(CamAnimating::Pan(response.drag_delta()));
                            } else if response.dragged_by(egui::PointerButton::Secondary) {
                                let prev_pos = centered_pos_raw - response.drag_delta();
                                let rot = centered_pos_raw.angle() - prev_pos.angle();
                                tab.tab_camera.camera.rotate(rot);

                                ui.ctx().animate_bool_with_time(cid, true, 0.0);
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
                                    .viewer_data.read()
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
                                        tab.viewer_data.read().persons[closest].position,
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
                        let class_colors = tab.viewer_data.read().modularity_classes.iter().map(|c| c.color.to_f32()).collect_vec();
                        let callback = egui::PaintCallback {
                            rect,
                            callback: Arc::new(egui_glow::CallbackFn::new(
                                move |_info, painter| {
                                    graph.write().paint(
                                        painter.gl(),
                                        cam,
                                        (edges, opac_edges),
                                        (nodes, opac_nodes),
                                        &class_colors,
                                    );
                                },
                            )),
                        };
                        ui.painter().add(callback);

                        let clipped_painter = ui.painter().with_clip_rect(rect);

                        let data = tab.viewer_data.read();
                        let draw_person = |id, color| {
                            let person: &Person = &data.persons[id];
                            let pos = person.position;
                            let pos_scr = (cam * Vector4::new(pos.x, pos.y, 0.0, 1.0)).xy();
                            let txt = WidgetText::from(person.name)
                                .background_color(color)
                                .color(Color32::WHITE);
                            let gal =
                                txt.into_galley(ui, Some(TextWrapMode::Extend), f32::INFINITY, TextStyle::Heading);
                            clipped_painter.add(CircleShape::filled(
                                rect.center() + vec2(pos_scr.x, -pos_scr.y) * rect.size() * 0.5,
                                7.0,
                                color,
                            ));
                            clipped_painter.add(TextShape::new(
                                rect.center()
                                    + vec2(pos_scr.x, -pos_scr.y) * rect.size() * 0.5
                                    + vec2(10.0, 10.0),
                                gal,
                                Color32::TRANSPARENT,
                            ));
                        };

                        if let Some(PathStatus::PathFound(ref path)) = tab.ui_state.path.path_status {
                            for (a, b) in path.iter().tuple_windows() {
                                let a = (cam * Vector4::from(data.persons[*a].position)).xy();
                                let b = (cam * Vector4::from(data.persons[*b].position)).xy();
                                clipped_painter.add(LineSegment {
                                    points: [rect.center() + vec2(a.x, -a.y) * rect.size() * 0.5, rect.center() + vec2(b.x, -b.y) * rect.size() * 0.5],
                                    stroke: PathStroke::new(2.0, Color32::from_rgba_unmultiplied(150, 0, 0, 200)),
                                });
                            }
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

    fn id(&mut self, tab: &mut Self::Tab) -> Id {
        tab.id
    }

    fn closeable(&mut self, tab: &mut Self::Tab) -> bool {
        tab.closeable
    }

    fn on_close(&mut self, tab: &mut Self::Tab) -> bool {
        if let GraphTabState::Loaded(ref mut tab) = tab.state {
            tab.rendered_graph
                .write()
                .destroy(&self.frame.gl().unwrap().clone());
        }
        true
    }
}

pub type NewTabRequest = GraphTab;

impl eframe::App for GraphViewApp {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
        let mut new_tab_request = None;

        if self.top_bar {
            self.show_top_bar(ctx);
        }

        let mut modal = Modal::new(ctx, "my_dialog");

        if let Ok(info) = self.modal.0.try_recv() {
            modal.dialog()
                .with_title(info.title)
                .with_body(info.body)
                .with_icon(Icon::Error)
                .open();
        }

        modal.show_dialog();

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
                            id: Id::new(("main_tab", chrono::Utc::now())),
                            closeable: false,
                            title: "Graphe".to_string(),
                            state: GraphTabState::loading(status_rx, state_rx, gl_mpsc),
                        }]),
                        string_tables: file.strings,
                    };
                    spawn_cancelable(self.modal.1.clone(), move || {
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
                ..
            } => {
                DockArea::new(tree)
                    .style({
                        let style = Style::from_egui(ctx.style().as_ref());
                        style
                    })
                    .show(
                        ctx,
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
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        egui::widgets::global_theme_preference_buttons(ui);
                        ui.add_space(15.0);
                        if ui.button("Réduire l'en-tête").clicked() {
                            self.top_bar = false;
                        }
                    });
                });
                ui.vertical(|ui| {
                    md!(ui, r#"
Si l'interface est **lente**:
- décocher **Afficher les liens**
- augmenter **Degré minimum**"#);
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

pub type GlTask = Box<dyn FnOnce(&mut RenderedGraph, &glow::Context) + Send + Sync + 'static>;

pub struct RenderedGraph {
    pub program_node: glow::Program,
    pub program_basic: glow::Program,
    pub program_edge: glow::Program,
    pub nodes_buffer: glow::Buffer,
    pub nodes_count: usize,
    pub nodes_array: glow::VertexArray,
    pub edges_count: usize,
    pub degree_filter: (u16, u16),
    pub filter_nodes: bool,
    pub destroyed: bool,
    pub tasks: VecDeque<GlTask>,
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
            const VERTS_PER_NODE: usize = 1;
            let node_vertices = viewer
                .persons
                .iter()
                .map(|p| {
                    crate::geom_draw::create_node_vertex(p)
                });
            const VERTS_PER_EDGE: usize = 6; // change this if below changes!
            let edge_vertices = edges
                .map(|e| {
                    let pa = &viewer.persons[e.a as usize];
                    let pb = &viewer.persons[e.b as usize];
                    (pa, pb)
                })
                //.filter(|(pa, pb)| pa.neighbors.len() > 5 && pb.neighbors.len() > 5)
                .flat_map(|(pa, pb)| {
                    crate::geom_draw::create_edge_vertices(pa, pb)
                });

            let vertices = node_vertices
                .chain(edge_vertices);

            #[cfg(target_arch = "wasm32")]
            let vertices = {
                const THRESHOLD: usize = 1024 * 1024 * 1024;
                const MAX_VERTS_IN_ONE_GIG: usize = THRESHOLD / std::mem::size_of::<PersonVertex>();
                let num_vertices = viewer.persons.len() * VERTS_PER_NODE + edges_count * VERTS_PER_EDGE;
                if num_vertices > MAX_VERTS_IN_ONE_GIG {
                    log!(status_tx, "More than {}MB of vertices ({}), truncating", THRESHOLD / 1024 / 1024, num_vertices);
                    vertices.take(MAX_VERTS_IN_ONE_GIG).collect_vec()
                } else {
                    log!(status_tx, "Less than {}MB of vertices ({}), not truncating", THRESHOLD / 1024 / 1024, num_vertices);
                    vertices.collect_vec()
                }
            };

            #[cfg(not(target_arch = "wasm32"))]
            let vertices = vertices.collect_vec();

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
                        vertices.len() * size_of::<PersonVertex>(),
                    ),
                    glow::STATIC_DRAW,
                );

                gl.vertex_attrib_pointer_f32(
                    0,
                    2,
                    glow::FLOAT,
                    false,
                    size_of::<PersonVertex>() as i32,
                    0,
                );
                gl.enable_vertex_attrib_array(0);
                gl.vertex_attrib_pointer_i32(
                    1,
                    1,
                    glow::UNSIGNED_INT,
                    size_of::<PersonVertex>() as i32,
                    size_of::<Point>() as i32,
                );
                gl.enable_vertex_attrib_array(1);

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
                    size_of::<Vertex>() as i32,
                    0,
                );
                gl.enable_vertex_attrib_array(0);
                gl.vertex_attrib_pointer_f32(
                    1,
                    3,
                    glow::UNSIGNED_BYTE,
                    true,
                    size_of::<Vertex>() as i32,
                    size_of::<Point>() as i32,
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
                degree_filter: (0, u16::MAX),
                filter_nodes: false,
                destroyed: false,
                tasks: VecDeque::new(),
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
            log::info!("Deleting arrays");
            gl.delete_vertex_array(self.nodes_array);
        }
    }

    fn paint(
        &mut self,
        gl: &glow::Context,
        cam: Matrix4<f32>,
        edges: (bool, f32),
        nodes: (bool, f32),
        class_colors: &[Color3f],
    ) {
        if self.destroyed {
            return;
        }

        while let Some(task) = self.tasks.pop_front() {
            task(self, gl);
        }

        use glow::HasContext as _;
        unsafe {
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

            gl.bind_vertex_array(Some(self.nodes_array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.nodes_buffer));

            let mut all_colors = [Color3f::new(0.0, 0.0, 0.0); 512];
            all_colors[..class_colors.len()].copy_from_slice(class_colors);

            if edges.0 {
                gl.use_program(Some(self.program_edge));
                gl.uniform_matrix_4_f32_slice(
                    Some(
                        &gl.get_uniform_location(self.program_edge, "u_projection")
                            .unwrap(),
                    ),
                    false,
                    cam.as_slice(),
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

                gl.uniform_3_f32_slice(
                    Some(
                        &gl.get_uniform_location(self.program_edge, "u_class_colors")
                            .unwrap(),
                    ),
                    unsafe { std::slice::from_raw_parts(all_colors.as_ptr() as *const f32, 512 * 3) },
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
                    cam.as_slice(),
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

                gl.uniform_3_f32_slice(
                    Some(
                        &gl.get_uniform_location(self.program_node, "u_class_colors")
                            .unwrap(),
                    ),
                    unsafe { std::slice::from_raw_parts(all_colors.as_ptr() as *const f32, 512 * 3) },
                );
                gl.draw_arrays(glow::POINTS, 0, self.nodes_count as i32);
            }
        }
    }
}
