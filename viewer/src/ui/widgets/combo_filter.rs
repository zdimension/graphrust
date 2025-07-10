use crate::app::{thread, ContextUpdater, ViewerData};
use eframe::emath::{vec2, Align2, NumExt, Rect, Vec2};
use eframe::epaint;
use eframe::epaint::{Shape, Stroke, StrokeKind};
use egui::style::WidgetVisuals;
use std::ops::Add;

use egui::{
    Align, Id, Layout, Painter, PopupCloseBehavior, Response, ScrollArea, SelectableLabel, Sense,
    Spinner, TextEdit, TextStyle, Ui, UiBuilder, WidgetText,
};

use crate::threading::MyRwLock;
use derivative::Derivative;
use eframe::epaint::text::TextWrapMode;
use egui::text::{CCursor, CCursorRange};
use std::sync::Arc;

/// Draws the dropdown icon (downwards arrow)
fn paint_icon(painter: &Painter, rect: Rect, visuals: &WidgetVisuals) {
    let rect = Rect::from_center_size(
        rect.center(),
        vec2(rect.width() * 0.7, rect.height() * 0.45),
    );

    painter.add(Shape::convex_polygon(
        vec![rect.left_top(), rect.right_top(), rect.center_bottom()],
        visuals.fg_stroke.color,
        Stroke::NONE,
    ));
}

fn button_frame(
    ui: &mut Ui,
    id: Id,
    is_popup_open: bool,
    sense: Sense,
    add_contents: impl FnOnce(&mut Ui),
) -> Response {
    let where_to_put_background = ui.painter().add(Shape::Noop);

    let margin = ui.spacing().button_padding;
    let interact_size = ui.spacing().interact_size;

    let mut outer_rect = ui.available_rect_before_wrap();
    outer_rect.set_height(outer_rect.height().at_least(interact_size.y));

    let inner_rect = outer_rect.shrink2(margin);
    let mut content_ui = ui.new_child(UiBuilder::new().max_rect(inner_rect).layout(*ui.layout()));
    add_contents(&mut content_ui);

    let mut outer_rect = content_ui.min_rect().expand2(margin);
    outer_rect.set_height(outer_rect.height().at_least(interact_size.y));

    let response = ui.interact(outer_rect, id, sense);

    if ui.is_rect_visible(outer_rect) {
        let visuals = if is_popup_open {
            &ui.visuals().widgets.open
        } else {
            ui.style().interact(&response)
        };

        ui.painter().set(
            where_to_put_background,
            epaint::RectShape::new(
                outer_rect.expand(visuals.expansion),
                visuals.corner_radius,
                visuals.weak_bg_fill,
                visuals.bg_stroke,
                StrokeKind::Inside,
            ),
        );
    }

    ui.advance_cursor_after_rect(outer_rect);

    response
}

pub const COMBO_WIDTH: f32 = 300.0;

const RESULTS: usize = 100;

/// Drop-down combobox with filtering
pub fn combo_with_filter(
    ui: &mut Ui,
    label: &str,
    current_item: &mut Option<usize>,
    viewer_data: &Arc<MyRwLock<ViewerData>>,
) -> Response {
    #[derive(Derivative, Clone)]
    #[derivative(Default)]
    struct ComboFilterState {
        #[derivative(Default(value = "(0..RESULTS).collect()"))]
        item_vector: Vec<usize>,
        loading: bool,
        pattern: String,
        first_open: bool,
    }

    type StateType = Arc<MyRwLock<ComboFilterState>>;
    let id = Id::new(label).with(ui.id()).with("combo_with_filter");

    let popup_id = id.with("popup");
    let wrap_enabled = false;
    let width = Some(COMBO_WIDTH);
    let is_popup_open = ui.memory(|m| m.is_popup_open(popup_id));
    if !is_popup_open {
        ui.memory_mut(|m| m.data.get_persisted_mut_or_default::<StateType>(id).clone())
            .write()
            .first_open = false;
    }

    let margin = ui.spacing().button_padding;
    let mut button_response = button_frame(ui, id, is_popup_open, Sense::click(), |ui| {
        let icon_spacing = ui.spacing().icon_spacing;
        // We don't want to change width when user selects something new
        let full_minimum_width = if wrap_enabled {
            // Currently selected value's text will be wrapped if needed, so occupy the available width.
            ui.available_width()
        } else {
            // Occupy at least the minimum width assigned to ComboBox.
            let width = width.unwrap_or_else(|| ui.spacing().combo_width);
            width - 2.0 * margin.x
        };
        let icon_size = Vec2::splat(ui.spacing().icon_width);
        let wrap_width = if wrap_enabled {
            // Use the available width, currently selected value's text will be wrapped if exceeds this value.
            ui.available_width() - icon_spacing - icon_size.x
        } else {
            // Use all the width necessary to display the currently selected value's text.
            f32::INFINITY
        };

        let (selected_text, dim) = match current_item {
            Some(value) => (
                WidgetText::from(viewer_data.read().persons[*value].name),
                false,
            ),
            None => (WidgetText::from(t!("Click here to search")), true),
        };

        let galley = selected_text.into_galley(
            ui,
            Some(if wrap_enabled {
                TextWrapMode::Wrap
            } else {
                TextWrapMode::Extend
            }),
            wrap_width,
            TextStyle::Button,
        );

        // The width necessary to contain the whole widget with the currently selected value's text.
        let width = if wrap_enabled {
            full_minimum_width
        } else {
            // Occupy at least the minimum width needed to contain the widget with the currently selected value's text.
            galley.size().x + icon_spacing + icon_size.x
        };

        // Case : wrap_enabled : occupy all the available width.
        // Case : !wrap_enabled : occupy at least the minimum width assigned to Slider and ComboBox,
        // increase if the currently selected value needs additional horizontal space to fully display its text (up to wrap_width (f32::INFINITY)).
        let width = width.at_least(full_minimum_width);
        let height = galley.size().y.max(icon_size.y);

        let (_, rect) = ui.allocate_space(Vec2::new(width, height));
        let button_rect = ui.min_rect().expand2(ui.spacing().button_padding);
        let response = ui.interact(button_rect, id, Sense::click());
        // response.active |= is_popup_open;

        if ui.is_rect_visible(rect) {
            let icon_rect = Align2::RIGHT_CENTER.align_size_within_rect(icon_size, rect);
            let visuals = if is_popup_open {
                &ui.visuals().widgets.open
            } else {
                ui.style().interact(&response)
            };

            paint_icon(ui.painter(), icon_rect.expand(visuals.expansion), visuals);

            let text_rect = Align2::LEFT_CENTER.align_size_within_rect(galley.size(), rect);
            ui.painter().galley(
                text_rect.min,
                galley,
                if dim {
                    visuals.text_color().gamma_multiply(0.5)
                } else {
                    visuals.text_color()
                },
            );
        }
    });

    if button_response.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(popup_id));
    }

    let mut sel_changed = false;
    let inner = egui::popup::popup_below_widget(
        ui,
        popup_id,
        &button_response,
        PopupCloseBehavior::CloseOnClick,
        |ui| {
            ui.vertical(|ui| {
                let binding =
                    ui.memory_mut(|m| m.data.get_persisted_mut_or_default::<StateType>(id).clone());

                let layout = Layout::centered_and_justified(ui.layout().main_dir());
                let txt_box_resp = ui.allocate_ui_with_layout(
                    ui.available_size() * vec2(1.0, 0.0),
                    layout,
                    |ui| {
                        let r = TextEdit::singleline(&mut binding.write().pattern).show(ui);
                        ui.add_space(2.0);
                        r
                    },
                );
                let mut txt_resp = txt_box_resp.inner;
                let txt = &txt_resp.response;

                let mut state = binding.write();
                if !state.first_open {
                    state.first_open = true;
                    ui.memory_mut(|m| m.request_focus(txt.id));
                    txt_resp.state.cursor.set_char_range(Some(CCursorRange::two(
                        CCursor::new(0),
                        CCursor::new(state.pattern.chars().count()),
                    )));
                    txt_resp.state.store(ui.ctx(), txt_resp.response.id);
                }
                let changed = txt.changed();

                if changed {
                    if state.pattern.is_empty() {
                        state.loading = false;
                        state.item_vector = ComboFilterState::default().item_vector;
                    } else {
                        state.loading = true;
                        let pattern = state.pattern.clone();
                        let engine = viewer_data.read().engine.clone();
                        let state = binding.clone();
                        let ctx = ContextUpdater::new(ui.ctx());
                        thread::spawn(move || {
                            let res = engine.get_blocking(|s| s.search(&pattern, RESULTS));
                            let mut state = state.write();
                            if state.pattern.eq(&pattern) {
                                state.item_vector = res.iter().map(|&i| i as usize).collect();
                                state.loading = false;
                                ctx.update();
                            }
                        });
                    }
                }

                let show_count = RESULTS.min(state.item_vector.len());

                let loading = state.loading;

                ScrollArea::vertical()
                    .max_height(ui.spacing().combo_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if show_count == 0 {
                            ui.add_enabled(
                                false,
                                SelectableLabel::new(false, t!("No results found")),
                            );
                        } else {
                            let data = viewer_data.read();
                            for i in 0..show_count.min(data.persons.len()) {
                                let idx = state.item_vector[i];

                                if ui
                                    .allocate_ui_with_layout(
                                        ui.available_size() * vec2(1.0, 0.0),
                                        Layout::centered_and_justified(ui.layout().main_dir())
                                            .with_cross_align(Align::LEFT),
                                        |ui| {
                                            ui.add_enabled(
                                                !loading,
                                                SelectableLabel::new(
                                                    *current_item == Some(idx),
                                                    data.persons[idx].name,
                                                ),
                                            )
                                        },
                                    )
                                    .inner
                                    .clicked()
                                {
                                    *current_item = Some(idx);
                                    sel_changed = true;
                                }
                            }
                        }
                    });

                if loading {
                    let rect = ui.min_rect();
                    let txt_rect = txt_box_resp.response.rect;
                    Spinner::new().paint_at(
                        ui,
                        Rect::from_center_size(
                            rect.center().add(vec2(0.0, txt_rect.height() / 2.0)),
                            vec2(20.0, 20.0),
                        ),
                    );
                }
            })
        },
    );
    if let Some(frame_r) = inner {
        if !sel_changed
            && !frame_r.response.clicked_elsewhere()
            && button_response.clicked_elsewhere()
        {
            ui.memory_mut(|mem| mem.open_popup(popup_id));
        }
    }

    if sel_changed {
        button_response.mark_changed();
    }

    button_response
}
