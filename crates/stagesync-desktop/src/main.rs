//! **StageSync** — the native desktop application.
//!
//! The window is a thin shell around [`stagesync_core`]: every measurement comes from
//! the core library, and this crate only presents the results and drives the long
//! running jobs on background threads.

use eframe::egui;
use stagesync_desktop::app;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    stagesync_desktop::jobs::install_panic_hook();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("StageSync")
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([960.0, 640.0])
            // Windows only delivers dropped files to windows that opted in.
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        "StageSync",
        options,
        Box::new(|_cc| Ok(Box::new(app::StageSyncApp::new()))),
    )
    .map_err(|error| anyhow::anyhow!("failed to start the StageSync window: {error}"))
}
