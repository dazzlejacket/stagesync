//! The parts of the application that differ between operating systems.
//!
//! Everything the desktop app knows about Windows, macOS or Linux lives here, so the
//! rest of the crate can be written once. The module uses nothing but `std`, so it
//! builds everywhere the application itself builds, and it is small enough to be
//! tested without a window.

use std::path::{Path, PathBuf};

/// The folder the settings live in, under the platform's configuration directory.
const APP_DIR: &str = "StageSync";

/// An environment variable that says where the settings live.
///
/// A portable installation can point this at the folder it was unpacked into, so
/// that the settings travel with it. A test points it at a temporary directory,
/// so that a test never touches the settings of whoever is running it.
pub const CONFIG_DIR_ENV: &str = "STAGESYNC_CONFIG_DIR";

/// The longest file name the application will produce.
///
/// Windows' own limit is 255 characters for the whole path, so a shorter cap leaves
/// room for a folder and a suffix.
const MAX_NAME_CHARS: usize = 120;

/// The longest extension that is still treated as an extension.
const MAX_EXTENSION_CHARS: usize = 16;

/// Where the settings live on this machine.
///
/// The location follows the platform's convention:
///
/// * Windows: `%APPDATA%\StageSync`, falling back to `%LOCALAPPDATA%\StageSync`;
/// * macOS: `~/Library/Application Support/StageSync`;
/// * Linux and the others: `$XDG_CONFIG_HOME/StageSync`, falling back to
///   `~/.config/StageSync`.
///
/// `STAGESYNC_CONFIG_DIR` overrides all of them, which is what a portable
/// installation and the tests use.
///
/// When neither the environment nor the home directory can be found, the settings go
/// to a folder under the temporary directory. That is not a good place for settings,
/// but it is better than refusing to start.
pub fn config_dir() -> PathBuf {
    if let Some(dir) = absolute_env(CONFIG_DIR_ENV) {
        return dir.join(APP_DIR);
    }
    platform_config_dir()
}

/// The platform's own configuration directory.
#[cfg(target_os = "windows")]
fn platform_config_dir() -> PathBuf {
    if let Some(dir) = absolute_env("APPDATA") {
        return dir.join(APP_DIR);
    }
    if let Some(dir) = absolute_env("LOCALAPPDATA") {
        return dir.join(APP_DIR);
    }
    fallback_config_dir()
}

/// The platform's own configuration directory.
#[cfg(target_os = "macos")]
fn platform_config_dir() -> PathBuf {
    match home_dir() {
        Some(home) => home
            .join("Library")
            .join("Application Support")
            .join(APP_DIR),
        None => fallback_config_dir(),
    }
}

/// The platform's own configuration directory.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn platform_config_dir() -> PathBuf {
    if let Some(dir) = absolute_env("XDG_CONFIG_HOME") {
        return dir.join(APP_DIR);
    }
    match home_dir() {
        Some(home) => home.join(".config").join(APP_DIR),
        None => fallback_config_dir(),
    }
}

/// The file the settings are written to.
pub fn settings_file() -> PathBuf {
    config_dir().join("settings.json")
}

/// Turn an arbitrary recording name into a file name that is legal everywhere.
///
/// Windows is the strictest of the three: it forbids `<>:"/\|?*`, control characters
/// and trailing dots or spaces, and it reserves a handful of device names. macOS and
/// Linux are more forgiving, but a name that is legal on all three is the only kind
/// worth producing.
pub fn sanitise_stem(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|character| {
            if matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            ) || character.is_control()
            {
                '_'
            } else {
                character
            }
        })
        .collect();

    let collapsed = collapse_whitespace(&replaced);
    // Windows refuses a name that ends in a dot or a space, so both ends are trimmed
    // before anything else is done with the name.
    let trimmed = collapsed.trim_matches(|c: char| c == ' ' || c == '.');
    let (stem, extension) = split_extension(trimmed);
    let stem = stem.trim_matches(|c: char| c == ' ' || c == '.');

    // The name is capped so that it always fits, with the extension kept: a recording
    // called `…wav` must still end in `wav`.
    let budget = MAX_NAME_CHARS.saturating_sub(extension.chars().count());
    let mut capped: String = stem.chars().take(budget).collect();
    // Cutting the name short can leave a trailing dot or space behind.
    while capped.ends_with(' ') || capped.ends_with('.') {
        capped.pop();
    }

    let stem = if capped.is_empty() {
        "untitled".to_string()
    } else if is_reserved_device_name(&capped) {
        format!("_{capped}")
    } else {
        capped
    };
    format!("{stem}{extension}")
}

/// A path as it should be shown to the user.
///
/// The path is printed as UTF-8, with the characters that are not valid UTF-8
/// replaced, and without the Windows verbatim prefix (`\\?\`), which is an internal
/// detail that only makes the path harder to read and to paste into another tool.
pub fn display_path(path: &Path) -> String {
    strip_verbatim_prefix(path).to_string_lossy().into_owned()
}

/// A path shortened to fit a narrow column, with the middle elided.
///
/// The tail of a path — the directory and the file name — is what identifies
/// the recording, so the *front* is what gets elided. Cutting the tail instead
/// would hide the name of the file that the row is about.
pub fn elide_path(path: &Path, max_chars: usize) -> String {
    let text = display_path(path);
    let count = text.chars().count();
    if count <= max_chars || max_chars < 8 {
        return text;
    }
    // Keep a little more of the tail than of the head: the file name matters
    // more than the drive or the home directory.
    let tail = max_chars * 2 / 3;
    let head = max_chars - tail - 1;
    let mut out = String::with_capacity(max_chars);
    out.extend(text.chars().take(head));
    out.push('\u{2026}');
    out.extend(text.chars().skip(count - tail));
    out
}

/// Remove the Windows verbatim (`\\?\`) prefix from a path.
///
/// On every other platform the path is returned unchanged, because no other platform
/// produces such a prefix.
#[cfg(windows)]
pub fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    let Some(rest) = text.strip_prefix(r"\\?\") else {
        return path.to_path_buf();
    };
    // `\\?\UNC\server\share` is the verbatim form of `\\server\share`.
    let rest = match rest.strip_prefix("UNC\\") {
        Some(unc) => format!(r"\\{unc}"),
        None => rest.to_string(),
    };
    PathBuf::from(rest)
}

/// Remove the Windows verbatim (`\\?\`) prefix from a path.
///
/// On every other platform the path is returned unchanged.
#[cfg(not(windows))]
pub fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    path.to_path_buf()
}

/// A short description of the platform this build is running on.
///
/// It is shown in the status line, so the user can tell which machine and which build
/// produced a result. The operating system and the architecture come from the
/// toolchain; the version of the operating system is not available through `std`
/// alone, and the application does not start a helper process to find out.
pub fn describe_platform() -> String {
    format!("{} {}", os_name(), arch_name())
}

/// Whether a file is one of the hidden ones that should be skipped.
///
/// A leading dot hides a file on macOS and Linux; Windows uses the `~$` prefix for
/// the temporary files that office applications leave behind.
pub fn is_hidden_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if name.starts_with('.') {
        return true;
    }
    is_temporary_office_file(name)
}

/// The name of the operating system, in the words a user would use.
fn os_name() -> &'static str {
    match std::env::consts::OS {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        "ios" => "iOS",
        "android" => "Android",
        "freebsd" => "FreeBSD",
        "openbsd" => "OpenBSD",
        "netbsd" => "NetBSD",
        other => other,
    }
}

/// The name of the architecture, in the words a user would use.
fn arch_name() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "x86",
        "arm" => "arm",
        other => other,
    }
}

/// The user's home directory, when it can be found.
///
/// Windows does not need it: the configuration directory there comes from the
/// environment.
#[cfg(not(target_os = "windows"))]
fn home_dir() -> Option<PathBuf> {
    absolute_env("HOME")
}

/// An environment variable that holds an absolute path, when it is set to one.
///
/// A relative path is ignored: the configuration directory has to be somewhere the
/// user can find it again, and a relative path would depend on the working
/// directory of whatever started the application.
fn absolute_env(name: &str) -> Option<PathBuf> {
    let value = std::env::var(name).ok()?;
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let path = PathBuf::from(value);
    if path.is_absolute() {
        Some(path)
    } else {
        None
    }
}

/// The configuration directory used when nothing better can be found.
fn fallback_config_dir() -> PathBuf {
    std::env::temp_dir().join(APP_DIR)
}

/// Replace every run of whitespace with a single space.
fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_space = false;
    for character in text.chars() {
        if character.is_whitespace() {
            if !in_space {
                out.push(' ');
                in_space = true;
            }
        } else {
            out.push(character);
            in_space = false;
        }
    }
    out
}

/// Split a name into the part before the extension and the extension, with its dot.
fn split_extension(name: &str) -> (&str, &str) {
    let Some(index) = name.rfind('.') else {
        return (name, "");
    };
    if index == 0 {
        return (name, "");
    }
    let extension = &name[index + 1..];
    if extension.is_empty()
        || extension.contains('.')
        || extension.chars().count() > MAX_EXTENSION_CHARS
    {
        return (name, "");
    }
    (&name[..index], &name[index..])
}

/// Whether a stem is one of the names Windows reserves for its own devices.
fn is_reserved_device_name(stem: &str) -> bool {
    let upper = stem.trim().to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    upper.len() == 4
        && (upper.starts_with("COM") || upper.starts_with("LPT"))
        && matches!(upper.as_bytes()[3], b'1'..=b'9')
}

/// Whether a name is one of the temporary files office applications leave behind.
#[cfg(windows)]
fn is_temporary_office_file(name: &str) -> bool {
    name.starts_with("~$")
}

/// Whether a name is one of the temporary files office applications leave behind.
///
/// The `~$` prefix is a Windows convention, so it is only honoured there.
#[cfg(not(windows))]
fn is_temporary_office_file(_name: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_configuration_directory_is_an_absolute_stage_sync_folder() {
        let dir = config_dir();
        assert!(dir.is_absolute(), "{} is not absolute", dir.display());
        assert_eq!(
            dir.file_name().and_then(|name| name.to_str()),
            Some(APP_DIR),
            "{} does not end in {APP_DIR}",
            dir.display()
        );
        assert_eq!(settings_file(), dir.join("settings.json"));
    }

    #[test]
    fn illegal_characters_become_underscores() {
        assert_eq!(
            sanitise_stem("a<b>c:d\"e/f\\g|h?i*j.wav"),
            "a_b_c_d_e_f_g_h_i_j.wav"
        );
    }

    #[test]
    fn control_characters_become_underscores() {
        assert_eq!(sanitise_stem("bad\u{0}name\u{7}.wav"), "bad_name_.wav");
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
        assert_eq!(sanitise_stem("recording. . "), "recording");
    }

    #[test]
    fn an_empty_name_becomes_untitled() {
        assert_eq!(sanitise_stem(""), "untitled");
        assert_eq!(sanitise_stem("   "), "untitled");
        assert_eq!(sanitise_stem("..."), "untitled");
    }

    #[test]
    fn the_reserved_device_names_are_escaped() {
        assert_eq!(sanitise_stem("CON"), "_CON");
        assert_eq!(sanitise_stem("con.wav"), "_con.wav");
        assert_eq!(sanitise_stem("PRN"), "_PRN");
        assert_eq!(sanitise_stem("AUX"), "_AUX");
        assert_eq!(sanitise_stem("NUL"), "_NUL");
        assert_eq!(sanitise_stem("COM1"), "_COM1");
        assert_eq!(sanitise_stem("com9.wav"), "_com9.wav");
        assert_eq!(sanitise_stem("LPT1"), "_LPT1");
        assert_eq!(sanitise_stem("lpt9"), "_lpt9");
    }

    #[test]
    fn the_names_that_only_look_reserved_are_left_alone() {
        assert_eq!(sanitise_stem("CONSOLE"), "CONSOLE");
        assert_eq!(sanitise_stem("COM0"), "COM0");
        assert_eq!(sanitise_stem("COM"), "COM");
        assert_eq!(sanitise_stem("NULL"), "NULL");
    }

    #[test]
    fn a_long_name_is_capped_and_keeps_its_extension() {
        let long = "a".repeat(400);
        let capped = sanitise_stem(&long);
        assert_eq!(capped.chars().count(), MAX_NAME_CHARS);

        let with_extension = sanitise_stem(&format!("{long}.wav"));
        assert_eq!(with_extension.chars().count(), MAX_NAME_CHARS);
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
            assert_eq!(shown, r"C:\music\reference.wav");
            assert!(!shown.starts_with(r"\\?\"));
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
    fn a_verbatim_unc_path_is_turned_back_into_a_unc_path() {
        let stripped = strip_verbatim_prefix(Path::new(r"\\?\UNC\server\share\a.wav"));
        #[cfg(windows)]
        assert_eq!(stripped, PathBuf::from(r"\\server\share\a.wav"));
        #[cfg(not(windows))]
        assert_eq!(stripped, Path::new(r"\\?\UNC\server\share\a.wav"));
    }

    #[test]
    fn a_ordinary_path_is_left_alone() {
        let path = Path::new("/music/reference.wav");
        assert_eq!(strip_verbatim_prefix(path), path);
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
        assert!(description.contains(os_name()));
        assert!(description.contains(arch_name()));
    }
}
