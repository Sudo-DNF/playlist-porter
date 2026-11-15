//! Bridge between the UI thread and the tokio runtime.
//!
//! The UI sends [`Command`]s over an unbounded tokio channel; the worker runs
//! all network, OAuth and file I/O on the runtime and sends [`Event`]s back
//! over a std channel, calling `ctx.request_repaint()` after each one. The UI
//! drains events with `try_recv` once per frame and never blocks.

use std::sync::mpsc;

use tokio::runtime::Handle;
use tokio::sync::mpsc as tmpsc;

use crate::core::config::{self, Config};
use crate::core::error::CoreError;

/// Requests from the UI.
#[derive(Debug)]
pub enum Command {
    /// Persist app credentials and UI prefs to `config.toml`.
    SaveConfig(Box<Config>),
}

/// Results and progress from the worker.
#[derive(Debug)]
pub enum Event {
    ConfigSaved,
    Error(CoreError),
}

/// Sends events to the UI and wakes it up.
#[derive(Clone)]
pub struct Sink {
    tx: mpsc::Sender<Event>,
    ctx: egui::Context,
}

impl Sink {
    pub fn send(&self, ev: Event) {
        // The receiver only disappears when the window is closing.
        let _ = self.tx.send(ev);
        self.ctx.request_repaint();
    }
}

pub struct WorkerHandle {
    tx: tmpsc::UnboundedSender<Command>,
    pub events: mpsc::Receiver<Event>,
}

impl WorkerHandle {
    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
    }
}

/// Start the worker loop on the app-owned runtime.
pub fn spawn(rt: &Handle, ctx: egui::Context) -> WorkerHandle {
    let (cmd_tx, mut cmd_rx) = tmpsc::unbounded_channel::<Command>();
    let (ev_tx, ev_rx) = mpsc::channel();
    let sink = Sink { tx: ev_tx, ctx };
    rt.spawn(async move {
        while let Some(cmd) = cmd_rx.recv().await {
            handle(cmd, &sink).await;
        }
    });
    WorkerHandle {
        tx: cmd_tx,
        events: ev_rx,
    }
}

async fn handle(cmd: Command, sink: &Sink) {
    match cmd {
        Command::SaveConfig(cfg) => {
            let res = tokio::task::spawn_blocking(move || config::save(&cfg)).await;
            match res {
                Ok(Ok(())) => sink.send(Event::ConfigSaved),
                Ok(Err(e)) => sink.send(Event::Error(e)),
                Err(e) => sink.send(Event::Error(CoreError::Io(e.to_string()))),
            }
        }
    }
}
