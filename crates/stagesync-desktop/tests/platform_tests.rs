//! Checks for the parts of the application that do not need a window: the platform
//! layer and the persisted settings.
//!
//! The platform layer is the only place in the crate that knows what operating
//! system it is running on, so these tests are what keeps it honest on the machine
//! they run on. The settings are checked against a temporary folder, so a test run
//! never touches the settings of the user who ran it.

use std::path::{Path, PathBuf};

use stagesync_desktop::platform::{
    config_dir, describe_platform, display_path, is_hidden_file, sanitise_stem, settings_file,
    strip_verbatim_prefix,
};
use stagesync_desktop::settings::AppSettings;

#[test]
fn the_settings_live_in_an_absolute_stage_sync_folder() {
    let dir = config_dir();
    assert!(dir.is_absolute(), "{} is not absolute", dir.display());
    assert_eq!(
        dir.file_name().and_then(|name| name.to_str()),
        Some("StageSync"),
        "{} does not end in StageSync",
        dir.display()
    );
    assert_eq!(settings_file(), dir.join("settings.json"));
}

#[test]
fn illegal_characters_are_replaced() {
    assert_eq!(
        sanitise_stem("a<b>c:d\"e/f\\g|h?i*j.wav"),
        "a_b_c_d_e_f_g_h_i_j.wav"
    );
    assert_eq!(sanitise_stem("bad\u{0}name.wav"), "bad_name.wav");
}

#[test]
fn whitespace_runs_collapse_and_the_edges_are_trimmed() {
    assert_eq!(
        sanitise_stem("  watch   recording .wav"),
        "watch recording.wav"
    );
    // A tab is a control character, so it is replaced rather than collapsed.
    assert_eq!(sanitise_stem("watch\trecording.wav"), "watch_recording.wav");
}

#[test]
fn trailing_dots_and_spaces_are_removed() {
    assert_eq!(sanitise_stem("recording..."), "recording");
    assert_eq!(sanitise_stem("recording   "), "recording");
}

#[test]
fn an_empty_name_becomes_untitled() {
    assert_eq!(sanitise_stem(""), "untitled");
    assert_eq!(sanitise_stem("   "), "untitled");
}

#[test]
fn the_reserved_device_names_are_escaped() {
    for name in ["CON", "PRN", "AUX", "NUL", "COM1", "COM9", "LPT1", "LPT9"] {
        let sanitised = sanitise_stem(name);
        assert!(
            sanitised.starts_with('_'),
            "{name} was not escaped: {sanitised}"
        );
    }
    assert_eq!(sanitise_stem("con.wav"), "_con.wav");
    assert_eq!(sanitise_stem("CONSOLE"), "CONSOLE");
}

#[test]
fn a_long_name_is_capped_and_keeps_its_extension() {
    let long = "a".repeat(400);
    let capped = sanitise_stem(&long);
    assert!(capped.chars().count() <= 120, "the name was not capped");

    let with_extension = sanitise_stem(&format!("{long}.wav"));
    assert!(
        with_extension.chars().count() <= 120,
        "the name with an extension was not capped"
    );
    assert!(with_extension.ends_with(".wav"));
}

#[test]
fn an_extension_is_kept() {
    assert_eq!(sanitise_stem("recording.wav"), "recording.wav");
    assert_eq!(sanitise_stem("recording"), "recording");
}

#[test]
fn the_windows_verbatim_prefix_is_removed() {
    let shown = display_path(Path::new(r"\\?\C:\music\reference.wav"));
    #[cfg(windows)]
    {
        assert!(
            !shown.starts_with(r"\\?\"),
            "the prefix was left in: {shown}"
        );
        assert_eq!(shown, r"C:\music\reference.wav");
    }
    #[cfg(not(windows))]
    {
        assert_eq!(
            shown,
            Path::new(r"\\?\C:\music\reference.wav").to_string_lossy()
        );
    }
}

#[test]
fn an_ordinary_path_is_left_alone() {
    let path = PathBuf::from("/music/reference.wav");
    assert_eq!(strip_verbatim_prefix(&path), path);
    assert_eq!(display_path(&path), "/music/reference.wav");
}

#[test]
fn hidden_files_are_recognised() {
    assert!(is_hidden_file(Path::new("/music/.hidden.wav")));
    assert!(!is_hidden_file(Path::new("/music/visible.wav")));
    #[cfg(windows)]
    assert!(is_hidden_file(Path::new(r"C:\music\~$temp.wav")));
    #[cfg(not(windows))]
    assert!(!is_hidden_file(Path::new("/music/~$temp.wav")));
}

#[test]
fn the_platform_is_described() {
    let description = describe_platform();
    assert!(!description.is_empty());
    assert!(
        description
            .to_lowercase()
            .contains(&std::env::consts::OS.to_lowercase()),
        "the description does not name the system: {description}"
    );
}

#[test]
fn the_settings_survive_a_round_trip() {
    let dir = tempfile::TempDir::new().expect("the temporary directory");
    let path = dir.path().join("settings.json");

    let settings = AppSettings {
        export_dir: "/music/exports".to_string(),
        layout: "timeline_merged".to_string(),
        sample_rate: Some(96_000),
        format: "int16".to_string(),
        use_spectral_check: false,
        measure_waveform_drift: false,
        force_offset_only: true,
        last_reference: "/music/reference.wav".to_string(),
        last_sources: vec!["/music/a.wav".to_string(), "/music/b.wav".to_string()],
        window_width: 1234.0,
        window_height: 567.0,
        ..AppSettings::default()
    };
    settings.save_to(&path).expect("the settings were written");
    assert!(path.exists(), "the settings file was not written");

    let loaded = AppSettings::load_from(&path);
    assert_eq!(loaded, settings);

    // The temporary file must not be left behind.
    assert!(!dir.path().join("settings.json.tmp").exists());
}

#[test]
fn a_damaged_settings_file_falls_back_to_the_defaults() {
    let dir = tempfile::TempDir::new().expect("the temporary directory");
    let path = dir.path().join("settings.json");
    std::fs::write(&path, "{ this is not json").expect("the file was written");

    assert_eq!(AppSettings::load_from(&path), AppSettings::default());
}

#[test]
fn a_missing_settings_file_falls_back_to_the_defaults() {
    let dir = tempfile::TempDir::new().expect("the temporary directory");
    let path = dir.path().join("nowhere.json");

    assert_eq!(AppSettings::load_from(&path), AppSettings::default());
}

#[test]
fn the_defaults_are_the_documented_ones() {
    let settings = AppSettings::default();
    assert!(settings.use_spectral_check);
    assert!(settings.measure_waveform_drift);
    assert!(!settings.force_offset_only);
    assert_eq!(settings.layout, "source_aligned");
    assert_eq!(settings.format, "float32");
    assert_eq!(settings.sample_rate, None);
}
