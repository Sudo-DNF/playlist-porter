//! The eframe application shell.

use tokio::runtime::Handle;

use super::worker::{self, Event, WorkerHandle};
use crate::core::config;

pub struct PorterApp {
    worker: WorkerHandle,
    status: String,
}

impl PorterApp {
    pub fn new(cc: &eframe::CreationContext<'_>, rt: Handle) -> Self {
        let worker = worker::spawn(&rt, cc.egui_ctx.clone());
        let status = match config::load() {
            Ok((cfg, _, _)) if cfg.has_spotify() => "Ready.".to_string(),
            Ok(_) => "Setup needed.".to_string(),
            Err(e) => e.to_string(),
        };
        Self { worker, status }
    }
}

impl eframe::App for PorterApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        while let Ok(ev) = self.worker.events.try_recv() {
            match ev {
                Event::ConfigSaved => self.status = "Saved.".into(),
                Event::Error(e) => self.status = e.to_string(),
            }
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("playlist-porter");
            ui.label(&self.status);
        });
    }
}
