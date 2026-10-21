use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

pub fn choose_open() -> Option<PathBuf> {
    FileDialog::new()
        .add_filter("Markdown and text", &["md", "txt", "markdown"])
        .pick_file()
}
pub fn choose_save(current: Option<&Path>) -> Option<PathBuf> {
    let mut dialog = FileDialog::new()
        .add_filter("Markdown", &["md"])
        .set_file_name(
            current
                .and_then(Path::file_name)
                .and_then(|n| n.to_str())
                .unwrap_or("Untitled.md"),
        );
    if let Some(parent) = current.and_then(Path::parent) {
        dialog = dialog.set_directory(parent);
    }
    dialog.save_file().map(|mut p| {
        if p.extension().is_none() {
            p.set_extension("md");
        }
        p
    })
}
pub fn choose_export(current: Option<&Path>) -> Option<PathBuf> {
    let mut dialog = FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(
        current
            .and_then(Path::file_stem)
            .and_then(|n| n.to_str())
            .map_or("Untitled.pdf".into(), |n| format!("{n}.pdf")),
    );
    if let Some(parent) = current.and_then(Path::parent) {
        dialog = dialog.set_directory(parent);
    }
    dialog.save_file().map(with_pdf_extension)
}
fn with_pdf_extension(p: PathBuf) -> PathBuf {
    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
        return p;
    }
    let mut name = p.into_os_string();
    name.push(".pdf");
    name.into()
}
pub fn confirm_unrendered(count: usize) -> bool {
    let description = if count == 1 {
        "1 formula hasn’t rendered yet and will be exported as its prompt text.".to_owned()
    } else {
        format!("{count} formulas haven’t rendered yet and will be exported as their prompt text.")
    };
    MessageDialog::new()
        .set_title("LVTeX")
        .set_description(description)
        .set_level(MessageLevel::Warning)
        .set_buttons(MessageButtons::OkCancel)
        .show()
        == MessageDialogResult::Ok
}
pub fn read(path: &Path) -> io::Result<String> {
    fs::read_to_string(path).map(|s| s.replace("\r\n", "\n").replace('\r', "\n"))
}
pub fn write(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    // Resolve symlinks, preserve permissions, and replace atomically on the same volume.
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
    let parent = path.parent().unwrap_or(Path::new("."));
    let temporary = parent.join(format!(
        ".lvtex-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let result = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        if let Ok(metadata) = fs::metadata(&path) {
            f.set_permissions(metadata.permissions())?;
        }
        f.write_all(contents.as_ref())?;
        f.sync_all()?;
        drop(f);
        fs::rename(&temporary, &path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
fn state_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library/Application Support/LVTeX/last-file"))
}
/// The per-user Lake project an installed app checks with, beside the other state files.
pub fn lean_dir() -> Option<PathBuf> {
    state_path().map(|p| p.with_file_name("lean"))
}
pub fn last() -> Option<PathBuf> {
    let p = PathBuf::from(fs::read_to_string(state_path()?).ok()?);
    p.is_file().then_some(p)
}
pub fn remember(path: Option<&Path>) {
    let Some(state) = state_path() else { return };
    let result = (|| {
        fs::create_dir_all(state.parent().unwrap())?;
        fs::write(
            state,
            path.map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )
    })();
    if let Err(error) = result {
        eprintln!("Cannot remember last document: {error}");
    }
}
fn mls_path() -> Option<PathBuf> {
    state_path().map(|p| p.with_file_name("mls"))
}
/// Whether Lean checking is on. It is on unless the switch file says `off`, so a new install checks.
pub fn mls_enabled() -> bool {
    mls_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .is_none_or(|text| parse_mls(&text))
}
/// The switch file's text as a choice: only `off`, once trimmed, turns checking off.
fn parse_mls(text: &str) -> bool {
    text.trim() != "off"
}
pub fn remember_mls_enabled(on: bool) {
    let Some(state) = mls_path() else { return };
    let result = (|| {
        fs::create_dir_all(state.parent().unwrap())?;
        write(&state, if on { "on\n" } else { "off\n" })
    })();
    if let Err(error) = result {
        eprintln!("Cannot remember Lean checking: {error}");
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowGeometry {
    pub width: f64,
    pub height: f64,
    pub position: Option<(f64, f64)>,
}
impl Default for WindowGeometry {
    fn default() -> Self {
        Self {
            width: 960.0,
            height: 760.0,
            position: None,
        }
    }
}
impl WindowGeometry {
    fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split_whitespace();
        let width: f64 = parts.next()?.parse().ok()?;
        let height: f64 = parts.next()?.parse().ok()?;
        if !(300.0..=10000.0).contains(&width) || !(240.0..=10000.0).contains(&height) {
            return None;
        }
        let position = parts.next().and_then(|x| {
            let x: f64 = x.parse().ok()?;
            let y: f64 = parts.next()?.parse().ok()?;
            (x.is_finite() && y.is_finite()).then_some((x, y))
        });
        Some(Self {
            width,
            height,
            position,
        })
    }
    fn encode(self) -> String {
        match self.position {
            Some((x, y)) => format!("{} {} {x} {y}\n", self.width, self.height),
            None => format!("{} {}\n", self.width, self.height),
        }
    }
}
pub fn window_geometry() -> WindowGeometry {
    state_path()
        .and_then(|p| fs::read_to_string(p.with_file_name("window-size")).ok())
        .and_then(|s| WindowGeometry::parse(&s))
        .unwrap_or_default()
}
pub fn remember_window_geometry(geometry: WindowGeometry) {
    let Some(state) = state_path().map(|p| p.with_file_name("window-size")) else {
        return;
    };
    let result = (|| {
        fs::create_dir_all(state.parent().unwrap())?;
        write(&state, geometry.encode())
    })();
    if let Err(error) = result {
        eprintln!("Cannot remember window geometry: {error}");
    }
}
pub fn error(action: &str, error: impl std::fmt::Display) {
    MessageDialog::new()
        .set_title("LVTeX")
        .set_description(format!("{action}\n\n{error}"))
        .set_level(MessageLevel::Error)
        .show();
}

#[cfg(test)]
#[path = "../tests/unit/file.rs"]
mod tests;
