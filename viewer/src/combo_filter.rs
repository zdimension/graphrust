// Ugly unsafe code ahead
// here be dragons

use crate::app::ViewerData;
use eframe::emath::{vec2, Align2, NumExt, Rect, Vec2};
use eframe::epaint;
use eframe::epaint::{Shape, Stroke};
use egui::style::WidgetVisuals;

use egui::{
    Align, Id, Layout, Painter, Response, ScrollArea, SelectableLabel, Sense, TextEdit, TextStyle,
    Ui, WidgetText,
};

use std::sync::{Arc, Mutex};

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
    let mut content_ui = ui.child_ui(inner_rect, *ui.layout());
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
                visuals.rounding,
                visuals.weak_bg_fill,
                visuals.bg_stroke,
            ),
        );
    }

    ui.advance_cursor_after_rect(outer_rect);

    response
}

pub const COMBO_WIDTH: f32 = 300.0;

/// Drop-down combobox with filtering
pub fn combo_with_filter(
    ui: &mut Ui,
    label: &str,
    current_item: &mut Option<usize>,
    viewer_data: &ViewerData<'_>,
) -> Response {
    #[derive(Default, Clone)]
    struct ComboFilterState {
        item_score_vector: Vec<(usize, isize)>,
        pattern: String,
        first_open: bool,
    }

    type StateType = Arc<Mutex<ComboFilterState>>;
    let id = Id::new(label).with("combo_with_filter");

    let popup_id = id.with("popup");
    let wrap_enabled = false;
    let width = Some(COMBO_WIDTH);
    let is_popup_open = ui.memory(|m| m.is_popup_open(popup_id));

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

        let selected_text = WidgetText::from(match current_item {
            Some(value) => viewer_data.persons[*value].name,
            None => "",
        });

        let galley =
            selected_text.into_galley(ui, Some(wrap_enabled), wrap_width, TextStyle::Button);

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
            ui.painter()
                .galley(text_rect.min, galley, visuals.text_color());
        }
    });

    if button_response.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(popup_id));
    }

    //let total_rect = button_response.rect.set_height(button_response.rect + )

    let mut sel_changed = false;
    let inner = egui::popup::popup_below_widget(ui, popup_id, &button_response, |ui| {
        ui.vertical(|ui| {
            let binding =
                ui.memory_mut(|m| m.data.get_persisted_mut_or_default::<StateType>(id).clone());
            let mut state = binding.lock().unwrap();

            let txt = ui.add_sized(
                ui.available_size() * vec2(1.0, 0.0),
                TextEdit::singleline(&mut state.pattern),
            );
            if !state.first_open {
                state.first_open = true;
                ui.memory_mut(|m| m.request_focus(txt.id));
            }
            let changed = txt.changed();
            let mut is_need_filter = false;

            if !state.pattern.is_empty() {
                is_need_filter = true;
            }

            if changed && is_need_filter {
                let res = viewer_data.engine.search(state.pattern.as_str());
                state.item_score_vector = res.iter().take(100).map(|i| (*i, 0_isize)).collect();
            }

            let show_count = 100.min(if is_need_filter {
                state.item_score_vector.len()
            } else {
                viewer_data.persons.len()
            });

            ScrollArea::vertical()
                .max_height(ui.spacing().combo_height)
                .show(ui, |ui| {
                    for i in 0..show_count {
                        let idx = if is_need_filter {
                            state.item_score_vector[i].0
                        } else {
                            i
                        };

                        if ui
                            .allocate_ui_with_layout(
                                ui.available_size() * vec2(1.0, 0.0),
                                Layout::centered_and_justified(ui.layout().main_dir())
                                    .with_cross_align(Align::LEFT),
                                |ui| {
                                    ui.add(SelectableLabel::new(
                                        *current_item == Some(idx),
                                        viewer_data.persons[idx].name,
                                    ))
                                },
                            )
                            .inner
                            .clicked()
                        {
                            *current_item = Some(idx);
                            sel_changed = true;
                        }
                    }
                });
        })
    });
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
