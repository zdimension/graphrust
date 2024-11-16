use derivative::Derivative;
use egui::{Align, Context, Id, Layout, WidgetText};
use egui_modal::{Icon, Modal, ModalStyle};
use std::sync::mpsc::{Receiver, Sender};

#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct ModalInfo {
    pub title: String,
    #[derivative(Debug(format_with = "fmt_modal_body"))]
    pub body: WidgetText,
}

fn fmt_modal_body(text: &WidgetText, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(text.text())
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

pub fn show_modal(ctx: &Context, recv: &Receiver<ModalInfo>, modal_id: &str) {
    let mut modal = Modal::new(ctx, modal_id).with_close_on_outside_click(true).with_style(&ModalStyle {
        default_width: Some(800.0),
        ..ModalStyle::default()
    });

    if let Ok(info) = recv.try_recv() {
        ctx.data_mut(|w| w.insert_temp(Id::new(modal_id).with("data"), info));
        modal.open();
    }

    if let Some(data) = ctx.data(|w| w.get_temp::<ModalInfo>(Id::new(modal_id).with("data"))) {
        modal.show(|ui| {
            modal.title(ui, data.title);
            modal.frame(ui, |ui| {
                modal.body_and_icon(ui, data.body, Icon::Error);
            });
            modal.buttons(ui, |ui| {
                ui.with_layout(Layout::top_down_justified(Align::Center), |ui| {
                    modal.button(ui, "OK");
                });
            });
        });
    }

    modal.show_dialog();
}