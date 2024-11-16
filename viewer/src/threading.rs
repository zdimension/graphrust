use crate::app::ContextUpdater;
use crate::thread;
use crate::ui::modal::{ModalInfo, ModalWriter};
use eframe::epaint::text::{LayoutJob, TextFormat};
use eframe::epaint::{FontFamily, FontId};
use egui::Context;
use parking_lot::lock_api::{RwLockReadGuard, RwLockWriteGuard};
use parking_lot::{RawRwLock, RwLock};
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};

#[derive(Debug)]
pub enum CancelableError {
    TabClosed,
    Other(anyhow::Error),
    Custom(Box<ModalInfo>),
}

impl<U> From<mpsc::SendError<U>> for CancelableError {
    fn from(_: mpsc::SendError<U>) -> CancelableError {
        CancelableError::TabClosed
    }
}

impl From<mpsc::RecvError> for CancelableError {
    fn from(_: mpsc::RecvError) -> CancelableError {
        CancelableError::TabClosed
    }
}

impl<T: IsNonSendError + Into<anyhow::Error>> From<T> for CancelableError {
    fn from(e: T) -> Self {
        CancelableError::Other(e.into())
    }
}

auto trait IsNonSendError {}

/// somehow it's not implemented even though it's auto
impl IsNonSendError for speedy::Error {}

impl<T> ! IsNonSendError for mpsc::SendError<T> {}
impl ! IsNonSendError for mpsc::RecvError {}

pub type Cancelable<T> = Result<T, CancelableError>;

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
                if chrono::Utc::now() - start > chrono::Duration::milliseconds(500) {
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

#[derive(Clone)]
pub struct StatusWriter {
    tx: Sender<StatusData>,
    ctx: ContextUpdater,
}

#[derive(Copy, Clone)]
pub struct Progress {
    pub max: usize,
    pub val: usize,
}

pub struct StatusReader {
    status: String,
    pub(crate) progress: Option<Progress>,
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

pub fn spawn_cancelable(ms: impl ModalWriter, f: impl FnOnce() -> Cancelable<()> + Send + 'static) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        match f() {
            Err(CancelableError::TabClosed) => {
                log::info!("Tab closed; cancelled");
            }
            Err(CancelableError::Other(e)) => {
                ms.send(ModalInfo {
                    title: t!("Error").to_string(),
                    body: {
                        let mut job = LayoutJob::default();
                        job.append(&t!("An error occurred:\n\n"), 0.0, TextFormat {
                            font_id: FontId::new(14.0, FontFamily::Proportional),
                            ..Default::default()
                        });
                        job.append(&format!("{:?}", e), 0.0, TextFormat {
                            font_id: FontId::new(11.0, FontFamily::Monospace),
                            ..Default::default()
                        });
                        job.into()
                    },
                });
            }
            Err(CancelableError::Custom(box info)) => {
                ms.send(info);
            }
            Ok(()) => {}
        }
    })
}