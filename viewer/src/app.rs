use crate::camera::Camera;
use std::marker::PhantomData;
use std::ops::Deref;

use crate::graph_storage::load_binary;
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
use nalgebra::{Matrix4, Vector4};
use simsearch::{SearchOptions, SimSearch};

use egui::epaint::TextShape;
use egui::Event::PointerButton;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Person<'a> {
    pub position: Point,
    pub size: f32,
    pub modularity_class: u16,
    pub id: &'a str,
    pub name: &'a str,
    pub neighbors: Vec<usize>,
}

impl<'a> Person<'a> {
    pub fn new(
        position: Point,
        size: f32,
        modularity_class: u16,
        id: &'a str,
        name: &'a str,
    ) -> Person<'a> {
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

#[derive(Clone)]
pub struct ViewerData<'a> {
    pub persons: Vec<Person<'a>>,
    pub modularity_classes: Vec<ModularityClass>,
    pub engine: SimSearch<usize>,
}

impl<'a> ViewerData<'a> {
    pub fn new(
        persons: Vec<Person<'a>>,
        modularity_classes: Vec<ModularityClass>,
    ) -> ViewerData<'a> {
        log::info!("Initializing search engine");
        let mut engine: SimSearch<usize> =
            SimSearch::new_with(SearchOptions::new().stop_words(vec!["-".into()]));
        for (i, person) in persons.iter().enumerate() {
            engine.insert(i, person.name);
        }
        ViewerData {
            persons,
            modularity_classes,
            engine,
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
}

pub struct GraphTab<'a> {
    pub ui_state: UiState,
    pub viewer_data: ViewerData<'a>,
    pub rendered_graph: Arc<Mutex<RenderedGraph>>,
    pub camera: Camera,
    pub cam_animating: Option<CamAnimating>,
    pub closeable: bool,
    pub title: String,
}

pub fn create_tab<'a, 'b>(
    title: impl Into<String>,
    viewer: ViewerData<'b>,
    edges: impl ExactSizeIterator<Item = &'a EdgeStore>,
    gl: &glow::Context,
    default_filter: u16,
    camera: Camera,
    ui_state: UiState,
) -> GraphTab<'b> {
    log::info!("Creating tab");
    let max_degree = viewer
        .persons
        .iter()
        .map(|p| p.neighbors.len())
        .max()
        .unwrap() as u16;
    log::info!("Max degree: {}", max_degree);
    let hide_edges = if cfg!(target_arch = "wasm32") {
        edges.len() > 300000
    } else {
        false
    };
    GraphTab {
        title: title.into(),
        closeable: true,
        camera,
        cam_animating: None,
        ui_state: UiState {
            display: DisplaySection {
                node_count: viewer.persons.len(),
                g_opac_edges: (300000.0 / edges.len() as f32).min(0.22),
                g_opac_nodes: ((40000.0 / viewer.persons.len() as f32)
                    * if hide_edges { 3.0 } else { 1.0 })
                .min(0.58),
                max_degree,
                g_show_edges: !hide_edges,
                ..DisplaySection::default()
            },
            ..ui_state
        },
        rendered_graph: Arc::new(Mutex::new(RenderedGraph {
            degree_filter: (default_filter, u16::MAX),
            ..RenderedGraph::new(gl, &viewer, edges)
        })),
        viewer_data: viewer,
    }
}

pub struct GraphViewApp<'graph> {
    top_bar: bool,
    tree: DockState<GraphTab<'graph>>,
    #[allow(dead_code)]
    // we do a little trolling
    string_tables: StringTables,
}

impl<'a> GraphViewApp<'a> {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let gl = cc
            .gl
            .as_ref()
            .expect("You need to run eframe with the glow backend");
        unsafe {
            gl.enable(glow::PROGRAM_POINT_SIZE);
        }
        let data = load_binary();
        let mut min = Point::new(f32::INFINITY, f32::INFINITY);
        let mut max = Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        for p in &data.viewer.persons {
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
        let default_tab = GraphTab {
            closeable: false,
            ..create_tab(
                "Graphe",
                data.viewer,
                data.edges.iter(),
                gl,
                17,
                cam,
                UiState::default(),
            )
        };
        Self {
            top_bar: true,
            tree: DockState::new(vec![default_tab]),
            string_tables: data.strings,
        }
    }
}

struct TabViewer<'graph, 'ctx, 'tab_request, 'frame> {
    ctx: &'ctx egui::Context,
    data: PhantomData<&'graph bool>,
    tab_request: &'tab_request mut Option<NewTabRequest<'graph>>,
    top_bar: &'tab_request mut bool,
    frame: &'frame mut eframe::Frame,
}

impl<'graph, 'ctx, 'tab_request, 'frame> egui_dock::TabViewer
    for TabViewer<'graph, 'ctx, 'tab_request, 'frame>
{
    type Tab = GraphTab<'graph>;

    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        RichText::from(&tab.title).into()
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        let ctx = self.ctx;
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
                    &tab.camera,
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
                if sz != tab.camera.size {
                    tab.camera.set_window_size(sz);
                }

                let response =
                    ui.interact(rect, id, egui::Sense::click().union(egui::Sense::drag()));

                if !response.is_pointer_button_down_on() {
                    if let Some(v) = tab.cam_animating {
                        let anim = ctx.animate_bool_with_time(cid, false, 0.5);
                        if anim == 0.0 {
                            tab.cam_animating = None;
                        } else {
                            match v {
                                CamAnimating::Pan(delta) => {
                                    tab.camera.pan(delta.x * anim, delta.y * anim);
                                }
                                CamAnimating::Rot(rot) => {
                                    tab.camera.rotate(rot * anim);
                                }
                            }
                        }
                    }
                }

                if let Some(pos) = response.interact_pointer_pos().or(response.hover_pos()) {
                    let centered_pos_raw = pos - rect.center();
                    let centered_pos = 2.0 * centered_pos_raw / rect.size();

                    if response.dragged_by(egui::PointerButton::Primary) {
                        tab.camera
                            .pan(response.drag_delta().x, response.drag_delta().y);

                        ctx.animate_bool_with_time(cid, true, 0.0);
                        tab.cam_animating = Some(CamAnimating::Pan(response.drag_delta()));
                    } else if response.dragged_by(egui::PointerButton::Secondary) {
                        let prev_pos = centered_pos_raw - response.drag_delta();
                        let rot = centered_pos_raw.angle() - prev_pos.angle();
                        tab.camera.rotate(rot);

                        ctx.animate_bool_with_time(cid, true, 0.0);
                        tab.cam_animating = Some(CamAnimating::Rot(rot));
                    }

                    let zero_pos = (pos - rect.min).to_pos2();

                    tab.ui_state.details.mouse_pos = Some(centered_pos.to_pos2());
                    let pos_world = (tab.camera.get_inverse_matrix()
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
                                    tab.ui_state.path.path_src = Some(closest);
                                    tab.ui_state.path.path_dirty = true;
                                    tab.ui_state.selected_user_field = SelectedUserField::PathDest;
                                }
                                SelectedUserField::PathDest => {
                                    tab.ui_state.path.path_dest = Some(closest);
                                    tab.ui_state.path.path_dirty = true;
                                }
                            }
                        }
                    }

                    let (scroll_delta, zoom_delta, multi_touch) =
                        ui.input(|is| (is.raw_scroll_delta, is.zoom_delta(), is.multi_touch()));

                    if scroll_delta.y != 0.0 {
                        let zoom_speed = 1.1;
                        let s = if scroll_delta.y > 0.0 {
                            zoom_speed
                        } else {
                            1.0 / zoom_speed
                        };
                        tab.camera.zoom(s, zero_pos);
                    }
                    if zoom_delta != 1.0 {
                        tab.camera.zoom(zoom_delta, zero_pos);
                    }

                    if let Some(multi_touch) = multi_touch {
                        tab.camera.rotate(multi_touch.rotation_delta);
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

                let cam = tab.camera.get_matrix();
                tab.ui_state.details.camera = cam;
                let callback = egui::PaintCallback {
                    rect,
                    callback: Arc::new(egui_glow::CallbackFn::new(move |_info, painter| {
                        graph.lock().unwrap().paint(
                            painter.gl(),
                            cam,
                            (edges, opac_edges),
                            (nodes, opac_nodes),
                        );
                    })),
                };
                ui.painter().add(callback);

                let draw_person = |id, color| {
                    let person: &Person<'_> = &tab.viewer_data.persons[id];
                    let pos = person.position;
                    let pos_scr = (cam * Vector4::new(pos.x, pos.y, 0.0, 1.0)).xy();
                    let txt = WidgetText::from(person.name)
                        .background_color(color)
                        .color(Color32::WHITE);
                    let gal = txt.into_galley(ui, Some(false), f32::INFINITY, TextStyle::Heading);
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

    fn closeable(&mut self, tab: &mut Self::Tab) -> bool {
        tab.closeable
    }

    fn on_close(&mut self, tab: &mut Self::Tab) -> bool {
        tab.rendered_graph
            .lock()
            .unwrap()
            .destroy(&self.frame.gl().unwrap().clone());
        true
    }
}

pub type NewTabRequest<'a> = GraphTab<'a>;

impl<'a> eframe::App for GraphViewApp<'a> {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let mut new_tab_request = None;

        if self.top_bar {
            self.show_top_bar(ctx);
        }

        DockArea::new(&mut self.tree)
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
            self.tree.push_to_focused_leaf(request);
        }
    }
}

impl<'a> GraphViewApp<'a> {
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
        gl: &glow::Context,
        viewer: &ViewerData<'_>,
        edges: impl ExactSizeIterator<Item = &'a EdgeStore>,
    ) -> Self {
        use glow::HasContext as _;

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

            log::info!("Compiling shaders");
            let [program_basic, program_edge, program_node] = programs.map(|shader_sources| {
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
            });

            let edges_count = edges.len();
            log::info!("Creating vertice list");
            let vertices = viewer
                .persons
                .iter()
                .map(|p| {
                    PersonVertex::new(
                        p.position,
                        viewer.modularity_classes[p.modularity_class as usize].color,
                        p.neighbors.len() as u16,
                        p.modularity_class,
                    )
                })
                .chain(
                    edges
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
                        }),
                )
                .collect_vec();

            log::info!("Sending data to GPU");
            let vertices_array = gl
                .create_vertex_array()
                .expect("Cannot create vertex array");
            gl.bind_vertex_array(Some(vertices_array));
            let vertices_buffer = gl.create_buffer().expect("Cannot create buffer");
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertices_buffer));
            log::info!("Buffering {} vertices", vertices.len());
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                std::slice::from_raw_parts(
                    vertices.as_ptr() as *const u8,
                    vertices.len() * std::mem::size_of::<PersonVertex>(),
                ),
                glow::STATIC_DRAW,
            );

            log::info!("Configuring buffers");

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

            log::info!("Creating path array");
            let path_array = gl
                .create_vertex_array()
                .expect("Cannot create vertex array");
            let path_buffer = gl.create_buffer().expect("Cannot create buffer");

            gl.bind_vertex_array(Some(path_array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(path_buffer));

            log::info!("Configuring path buffers");
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
            log::info!("Done");

            Self {
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
            }
        }
    }

    fn destroy(&mut self, gl: &glow::Context) {
        self.destroyed = true;
        use glow::HasContext as _;
        unsafe {
            gl.delete_program(self.program_basic);
            gl.delete_program(self.program_edge);
            gl.delete_program(self.program_node);
            gl.delete_buffer(self.nodes_buffer);
            gl.delete_vertex_array(self.nodes_array);
            gl.delete_buffer(self.path_buffer);
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
