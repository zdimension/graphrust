use crate::app::{GraphTabState, Person, ViewerData};
use crate::graph_render::camera::{CamXform, Camera};
use crate::graph_render::{GlForwarder, RenderedGraph};
use crate::threading::{Cancelable, MyRwLock, StatusWriter};
use crate::ui::modal::ModalInfo;
use crate::ui::sections::display;
use crate::ui::sections::path::PathStatus;
use crate::ui::{SelectedUserField, UiState};
use crate::{app, log};
use eframe::egui_glow;
use eframe::emath::{vec2, Align, Vec2};
use eframe::epaint::text::TextWrapMode;
use eframe::epaint::Shape::LineSegment;
use eframe::epaint::{CircleShape, Color32, PathStroke, Stroke, TextShape};
use egui::{emath, pos2, Id, Layout, Rect, RichText, TextStyle, Ui, WidgetText};
use graph_format::nalgebra::{Similarity3, Vector4};
use graph_format::EdgeStore;
use itertools::Itertools;
use std::ops::Deref;
use std::sync::mpsc::Sender;
use std::sync::Arc;

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
    pub viewer_data: Arc<MyRwLock<ViewerData>>,
    pub rendered_graph: Arc<MyRwLock<RenderedGraph>>,
    pub tab_camera: TabCamera,
}

pub struct GraphTab {
    pub id: Id,
    pub title: String,
    pub closeable: bool,
    pub state: GraphTabState,
}

pub fn create_tab<'a>(
    viewer: ViewerData,
    edges: impl ExactSizeIterator<Item = &'a EdgeStore>,
    gl: GlForwarder,
    default_filter: u16,
    camera: Camera,
    ui_state: UiState,
    status_tx: StatusWriter,
) -> Cancelable<GraphTabLoaded> {
    log!(
        status_tx,
        t!(
            "Creating tab with %{n} nodes, %{m} edges, and %{c} classes",
            n = viewer.persons.len(),
            m = edges.len(),
            c = viewer.modularity_classes.len()
        )
    );
    log!(status_tx, t!("Computing maximum degree..."));
    let max_degree = viewer
        .persons
        .iter()
        .map(|p| p.neighbors.len())
        .max()
        .unwrap() as u16;
    log!(status_tx, t!("Maximum degree is %{d}", d = max_degree));
    Ok(GraphTabLoaded {
        tab_camera: TabCamera {
            camera,
            camera_default: camera,
            cam_animating: None,
        },
        ui_state: UiState {
            display: display::DisplaySection {
                g_opac_edges: (400000.0 / edges.len() as f32).min(0.22),
                g_opac_nodes: ((70000.0 / viewer.persons.len() as f32) * 2.0).min(0.58),
                max_degree,
                ..Default::default()
            },
            ..ui_state
        },
        rendered_graph: Arc::new(MyRwLock::new({
            let mut graph = RenderedGraph::new(gl, &viewer, edges, status_tx)?;
            graph.node_filter.degree_filter = (default_filter, u16::MAX);
            graph
        })),
        viewer_data: Arc::from(MyRwLock::new(viewer)),
    })
}

pub struct TabViewer<'tab_request, 'frame> {
    pub tab_request: &'tab_request mut Option<NewTabRequest>,
    pub top_bar: &'tab_request mut bool,
    pub frame: &'frame mut eframe::Frame,
    pub modal: Sender<ModalInfo>,
}

impl egui_dock::TabViewer for TabViewer<'_, '_> {
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
                if let Ok(work) = gl_mpsc.0.try_recv() {
                    work.0(self.frame.gl().unwrap().deref(), &gl_mpsc.1);
                }
                app::show_status(ui, status_rx);
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

                        let response =
                            ui.interact(rect, id, egui::Sense::click().union(egui::Sense::drag()));

                        if !response.is_pointer_button_down_on() {
                            if let Some(v) = tab.tab_camera.cam_animating {
                                const DUR: f32 = 0.5;
                                let anim = ui.ctx().animate_bool_with_time_and_easing(
                                    cid,
                                    false,
                                    DUR,
                                    emath::easing::circular_out,
                                );
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
                                            tab.tab_camera
                                                .camera
                                                .pan(delta.x * anim, delta.y * anim);
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

                                            tab.tab_camera.camera.transf =
                                                Similarity3::from_isometry(
                                                    from.isometry.lerp_slerp(&to.isometry, t),
                                                    lerp(from.scaling(), to.scaling(), t),
                                                );
                                        }
                                    }
                                }
                            }
                        }

                        let fixed_cam = tab.tab_camera.camera.with_window_size(sz);

                        if let Some(pos) = response.interact_pointer_pos().or(response.hover_pos())
                        {
                            let centered_pos_raw = pos - rect.center();
                            let centered_pos = 2.0 * centered_pos_raw / rect.size();

                            if response.dragged_by(egui::PointerButton::Primary) {
                                let delta = response.drag_delta() / Camera::get_major_axis(sz);
                                tab.tab_camera.camera.pan(delta.x, delta.y);

                                ui.ctx().animate_bool_with_time(cid, true, 0.0);
                                tab.tab_camera.cam_animating = Some(CamAnimating::Pan(delta));
                            } else if response.dragged_by(egui::PointerButton::Secondary) {
                                let prev_pos = centered_pos_raw - response.drag_delta();
                                let rot = centered_pos_raw.angle() - prev_pos.angle();
                                tab.tab_camera.camera.rotate(rot);

                                ui.ctx().animate_bool_with_time(cid, true, 0.0);
                                tab.tab_camera.cam_animating = Some(CamAnimating::Rot(rot));
                            }

                            tab.ui_state.details.mouse_pos = Some(centered_pos.to_pos2());
                            let pos_world = (fixed_cam.get_inverse_matrix()
                                * Vector4::new(centered_pos.x, -centered_pos.y, 0.0, 1.0))
                            .xy();
                            tab.ui_state.details.mouse_pos_world = Some(pos_world);

                            let zero_pos = pos2(centered_pos_raw.x, centered_pos_raw.y)
                                / Camera::get_major_axis(sz);

                            if response.clicked() {
                                let closest = tab
                                    .viewer_data
                                    .read()
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
                                            tab.ui_state.path.path_settings.path_src =
                                                Some(closest);
                                            tab.ui_state.path.path_dirty = true;
                                            tab.ui_state.selected_user_field =
                                                SelectedUserField::PathDest;
                                        }
                                        SelectedUserField::PathDest => {
                                            tab.ui_state.path.path_settings.path_dest =
                                                Some(closest);
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

                        let cam = fixed_cam.get_matrix();
                        let class_colors = tab
                            .viewer_data
                            .read()
                            .modularity_classes
                            .iter()
                            .map(|c| c.color.to_u32())
                            .collect_vec();
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
                            let gal = txt.into_galley(
                                ui,
                                Some(TextWrapMode::Extend),
                                f32::INFINITY,
                                TextStyle::Heading,
                            );
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

                        let alpha = if tab.ui_state.path.path_loading {
                            Color32::from_white_alpha(30)
                        } else {
                            Color32::from_white_alpha(255)
                        };

                        let path = if let Some(PathStatus::PathFound(ref path)) =
                            tab.ui_state.path.path_status
                        {
                            for (a, b) in path.iter().tuple_windows() {
                                let a = (cam * Vector4::from(data.persons[*a].position)).xy();
                                let b = (cam * Vector4::from(data.persons[*b].position)).xy();
                                clipped_painter.add(LineSegment {
                                    points: [
                                        rect.center() + vec2(a.x, -a.y) * rect.size() * 0.5,
                                        rect.center() + vec2(b.x, -b.y) * rect.size() * 0.5,
                                    ],
                                    stroke: Stroke::new(
                                        2.0,
                                        Color32::from_rgba_unmultiplied(150, 0, 0, 200) * alpha,
                                    ),
                                });
                            }
                            path
                        } else {
                            &tab.ui_state
                                .path
                                .path_settings
                                .path_src
                                .iter()
                                .chain(tab.ui_state.path.path_settings.path_dest.iter())
                                .copied()
                                .collect_vec()
                        };
                        for &p in path {
                            draw_person(p, Color32::from_rgba_unmultiplied(150, 0, 0, 200) * alpha);
                        }

                        if let Some(sel) = tab.ui_state.infos.infos_current {
                            draw_person(sel, Color32::from_rgba_unmultiplied(0, 100, 0, 200));
                        }

                        ui.style_mut().text_styles.insert(
                            TextStyle::Button,
                            egui::FontId::new(24.0, eframe::epaint::FontFamily::Proportional),
                        );
                        const PADDING: f32 = 4.0;
                        const BUTTON_SIZE: f32 = 30.0;
                        if ui
                            .put(
                                Rect::from_min_size(
                                    rect.max - vec2(BUTTON_SIZE + PADDING, BUTTON_SIZE + PADDING),
                                    vec2(BUTTON_SIZE, BUTTON_SIZE),
                                ),
                                egui::Button::new("⌖"),
                            )
                            .on_hover_text(t!("Center camera"))
                            .clicked()
                        {
                            ui.ctx().animate_bool_with_time(cid, true, 0.0);
                            let camera = &mut tab.tab_camera;
                            camera.cam_animating = Some(CamAnimating::PanTo {
                                from: camera.camera.transf,
                                to: camera.camera_default.transf,
                            });
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
