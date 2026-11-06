//! **StageSync** — the native desktop application.
//!
//! The window is a thin shell around [`stagesync_core`]: every measurement comes from
//! the core library, and this crate only presents the results and drives the long
//! running jobs on background threads.
//!
//! ## Command line
//!
//! ```text
//! stagesync-desktop [--analyse] [FILE…]
//! ```
//!
//! * `FILE…` — recordings to open when the window appears. The first one becomes
//!   the reference and the rest become targets, exactly as if they had been
//!   dropped onto the window.
//! * `--analyse` — start the analysis as soon as the window appears, instead of
//!   waiting for the Analyze button to be pressed.
//!
//! The switches exist so that the application can be driven from a script — a
//! recording session that starts the machine and leaves it alone, or a test that
//! needs the window to do the same thing every time.

use eframe::egui;
use stagesync_desktop::app;
use stagesync_desktop::settings::AppSettings;

/// What the command line asked for.
struct Arguments {
    /// The recordings to open.
    files: Vec<std::path::PathBuf>,
    /// Whether the analysis should start on its own.
    analyse: bool,
}

/// Read the command line.
///
/// An unknown switch is an error rather than a file name: a mistyped switch must
/// not silently become a recording that cannot be found.
fn parse_arguments() -> anyhow::Result<Arguments> {
    let mut files = Vec::new();
    let mut analyse = false;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--analyse" | "--analyze" => analyse = true,
            "--help" | "-h" => {
                println!(
                    "StageSync — align concert recordings against a reference.\n\n\
                     Usage: stagesync-desktop [--analyse] [FILE…]\n\n\
                     \x20 FILE…      recordings to open; the first is the reference\n\
                     \x20 --analyse  start the analysis when the window opens"
                );
                std::process::exit(0);
            }
            other => {
                if other.starts_with('-') {
                    anyhow::bail!("unknown option {other}\nTry --help.");
                }
                files.push(std::path::PathBuf::from(other));
            }
        }
    }
    Ok(Arguments { files, analyse })
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    stagesync_desktop::jobs::install_panic_hook();

    let arguments = parse_arguments()?;

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

    let files = arguments.files;
    let analyse = arguments.analyse;

    eframe::run_native(
        "StageSync",
        options,
        Box::new(move |_cc| {
            let mut application = app::StageSyncApp::new();
            if !files.is_empty() {
                application.open_files(files);
            }
            if analyse {
                application.analyse_on_start();
            }
            Ok(Box::new(application))
        }),
    )
    .map_err(|error| anyhow::anyhow!("failed to start the StageSync window: {error}"))
}
