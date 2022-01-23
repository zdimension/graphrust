use std::ffi::CString;
use imgui::sys::{ImU32, ImVec2};
use imgui::Ui;
use crate::{FONT_SIZE, ViewerData};

fn add(a: ImVec2, b: ImVec2) -> ImVec2
{
    ImVec2 { x: a.x + b.x, y: a.y + b.y }
}

unsafe fn render_arrow(draw_list: *mut imgui::sys::ImDrawList, pos: ImVec2, col: ImU32, scale: f32)
{
    let h = FONT_SIZE * 1.0;
    let r = h * 0.40 * scale;
    let center = add(pos, ImVec2 { x: h * 0.50, y: h * 0.50 * scale });

    let a = ImVec2 { x: 0.000 * r, y: 0.750 * r };
    let b = ImVec2 { x: -0.866 * r, y: -0.750 * r };
    let c = ImVec2 { x: 0.866 * r, y: -0.750 * r };

    imgui::sys::ImDrawList_AddTriangleFilled(draw_list, add(center, a), add(center, b), add(center, c), col);
}

pub fn combo_with_filter<'a>(ui: &Ui, label: &str, current_item: &mut Option<usize>, viewer_data: &ViewerData<'a>) -> bool
{
    unsafe {
        let storage = imgui::sys::igGetStateStorage();
        let id = imgui::sys::igGetID_Str(label.as_ptr() as _);

        struct ComboFilterData
        {
            item_score_vector: Vec<(usize, isize)>,
            pattern: String,
        }

        let mut cfdata = imgui::sys::ImGuiStorage_GetVoidPtr(storage, id) as *mut ComboFilterData;
        if cfdata.is_null()
        {
            let vec = ComboFilterData {
                item_score_vector: Vec::new(),
                pattern: String::new(),
            };
            cfdata = Box::into_raw(Box::new(vec)) as _;
            imgui::sys::ImGuiStorage_SetVoidPtr(storage, id, cfdata as _);
        }

        let preview_value = match current_item
        {
            Some(value) => viewer_data.persons[*value].name,
            None => "",
        };

        let mut is_need_filter = false;

        let combo_button_name = format!("{}##name_ComboWithFilter_button_{}", preview_value, label);

        let name_popup = format!("##name_popup_{}", label);

        let mut value_changed = false;

        let expected_w = imgui::sys::igCalcItemWidth();
        let mut item_min = imgui::sys::ImVec2 { x: 0.0, y: 0.0 };
        imgui::sys::igGetItemRectMin(&mut item_min);
        let mut is_new_open = false;
        let sz = imgui::sys::igGetFrameHeight();
        let size = imgui::sys::ImVec2 { x: sz, y: sz };

        let mut style = imgui::sys::igGetStyle();
        let button_text_align_x = (*style).ButtonTextAlign.x;
        (*style).ButtonTextAlign.x = 0.0;
        if ui.button_with_size(combo_button_name, [expected_w, 0.0])
        {
            ui.open_popup(&name_popup);
            is_new_open = true;
        }
        (*style).ButtonTextAlign.x = button_text_align_x;
        let mut content_min = imgui::sys::ImVec2 { x: 0.0, y: 0.0 };
        imgui::sys::igGetItemRectMin(&mut content_min);
        let pos = add(content_min, imgui::sys::ImVec2 { x: expected_w - sz, y: 0.0 });
        let text_col = imgui::sys::igGetColorU32_Col(imgui::sys::ImGuiCol_Text as i32, 1.0);
        render_arrow(
            imgui::sys::igGetWindowDrawList(),
            add(pos, ImVec2 { x: 0f32.max((size.x - FONT_SIZE) * 0.5), y: 0f32.max((size.y - FONT_SIZE) * 0.5) }),
            text_col,
            1.0);
        let mut item_max = imgui::sys::ImVec2 { x: 0.0, y: 0.0 };
        imgui::sys::igGetItemRectMax(&mut item_max);

        imgui::sys::igSetNextWindowPos(
            ImVec2 { x: content_min.x, y: item_max.y },
            imgui::sys::ImGuiCond_None as i32,
            ImVec2 { x: 0.0, y: 0.0 });
        let mut item_rect_size = imgui::sys::ImVec2 { x: 0.0, y: 0.0 };
        imgui::sys::igGetItemRectSize(&mut item_rect_size);
        imgui::sys::igSetNextWindowSize(
            ImVec2 { x: item_rect_size.x, y: 0.0 },
            imgui::sys::ImGuiCond_None as i32);
        ui.popup(name_popup, ||
            {
                imgui::sys::igPushStyleColor_Vec4(imgui::sys::ImGuiCol_FrameBg as i32, imgui::sys::ImVec4 { x: 240.0 / 255.0, y: 240.0 / 255.0, z: 240.0 / 255.0, w: 255.0 });
                imgui::sys::igPushStyleColor_Vec4(imgui::sys::ImGuiCol_Text as i32, imgui::sys::ImVec4 { x: 0.0, y: 0.0, z: 0.0, w: 255.0 });
                imgui::sys::igPushItemWidth(-f32::MIN_POSITIVE);

                if is_new_open
                {
                    imgui::sys::igSetKeyboardFocusHere(0);
                }

                let changed = ui.input_text("##ComboWithFilter_inputText", &mut (*cfdata).pattern).build();

                imgui::sys::igPopStyleColor(2);
                if !(*cfdata).pattern.is_empty()
                {
                    is_need_filter = true;
                }

                if changed && is_need_filter
                {
                    let res = viewer_data.engine.search((*cfdata).pattern.as_str());
                    (*cfdata).item_score_vector = res.iter()
                        .take(100)
                        .map(|i| (*i, 0 as isize))
                        .collect();
                }

                let show_count = 100.min(if is_need_filter { (*cfdata).item_score_vector.len() } else { viewer_data.persons.len() });
                let name = CString::new("##ComboWithFilter_itemList").unwrap();
                let height_in_items_f = show_count.min(7) as f32 + 0.25;
                if imgui::sys::igBeginListBox(
                    name.as_ptr(),
                    ImVec2 { x: 0.0, y: imgui::sys::igGetTextLineHeightWithSpacing() * height_in_items_f + (*style).FramePadding.y * 2.0 })
                {
                    for i in 0..show_count
                    {
                        let idx = if is_need_filter {
                            (*cfdata).item_score_vector[i].0
                        } else {
                            i
                        };
                        imgui::sys::igPushID_Int(idx as i32);
                        let item_selected = Some(idx) == *current_item;
                        let item_text = CString::new(viewer_data.persons[idx].name).expect("What");
                        if imgui::sys::igSelectable_Bool(item_text.as_ptr(), item_selected, 0, ImVec2 { x: 0.0, y: 0.0 })
                        {
                            value_changed = true;
                            *current_item = Some(idx);
                            ui.close_current_popup();
                        }
                        if item_selected
                        {
                            imgui::sys::igSetItemDefaultFocus();
                        }
                        imgui::sys::igPopID();
                    }
                    imgui::sys::igEndListBox();
                }
                imgui::sys::igPopItemWidth();
            });

        value_changed
    }
}