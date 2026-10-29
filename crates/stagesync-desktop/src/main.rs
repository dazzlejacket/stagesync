//! **StageSync** — the native desktop application.
//!
//! The window is a thin shell around [`stagesync_core`]: every measurement comes from
//! the core library, and this crate only presents the results and drives the long
//! running jobs on background threads.

use eframe::egui;
use stagesync_desktop::app;
use stagesync_desktop::settings::AppSettings;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    stagesync_desktop::jobs::install_panic_hook();

    // The window opens at the size it was last closed at.
    let stored = AppSettings::load();
    let width = stored.window_width.clamp(960.0, 7_680.0);
    let height = stored.window_height.clamp(640.0, 4_320.0);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("StageSync")
            .with_inner_size([width, height])
            .with_min_inner_size([960.0, 640.0])
            // Files dropped onto the window arrive through egui's raw input, which
            // works on macOS and Linux as well as on Windows.
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
