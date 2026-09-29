use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk::gio::prelude::*;
use gtk::{gio, glib};
use serde::{Deserialize, Serialize};

pub const APP_ID: &str = "io.github.smlblr.GnomeIdleScreensaver";
pub const DAEMON_APP_ID: &str = "io.github.smlblr.GnomeIdleScreensaver.Daemon";

pub const MIN_IDLE_MINUTES: u32 = 1;
pub const MAX_IDLE_MINUTES: u32 = 720;

const CONFIG_ENV: &str = "GNOME_IDLE_SCREENSAVER_CONFIG";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub enabled: bool,
    pub idle_minutes: u32,
    /// Do not start while a session idle inhibitor (flag 8) exists, e.g. a playing video.
    pub respect_inhibitors: bool,
    /// An id from `animations::ALL`.
    pub animation: String,
    /// "auto" (the desktop's language), "en" or "tr"; see i18n.rs.
    pub language: String,
    /// Option tables keyed by animation id; each module parses its own table,
    /// so adding a module or an option never touches this struct.
    #[serde(skip_serializing_if = "toml::Table::is_empty")]
    pub animations: toml::Table,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            idle_minutes: 5,
            respect_inhibitors: true,
            animation: crate::animations::ALL[0].id.to_owned(),
            language: "auto".to_owned(),
            animations: toml::Table::new(),
        }
    }
}

impl Config {
    pub fn idle_ms(&self) -> u64 {
        u64::from(self.idle_minutes) * 60_000
    }

    pub fn animation_options(&self, id: &str) -> Option<&toml::Value> {
        self.animations.get(id)
    }

    fn sanitized(mut self) -> Self {
        self.idle_minutes = self.idle_minutes.clamp(MIN_IDLE_MINUTES, MAX_IDLE_MINUTES);
        self
    }
}

pub fn path() -> PathBuf {
    match std::env::var_os(CONFIG_ENV) {
        Some(p) => PathBuf::from(p),
        None => glib::user_config_dir()
            .join("gnome-idle-screensaver")
            .join("config.toml"),
    }
}

/// A missing file means defaults; only unreadable or malformed files are errors.
pub fn load() -> Result<Config, String> {
    let path = path();
    match std::fs::read_to_string(&path) {
        Ok(text) => toml::from_str::<Config>(&text)
            .map(Config::sanitized)
            .map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

pub fn save(config: &Config) -> Result<(), String> {
    let path = path();
    let err = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| err(&e))?;
    }
    let text = toml::to_string_pretty(config).map_err(|e| err(&e))?;
    // Rename over the old file so the daemon's monitor never reads a half-written one.
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text).map_err(|e| err(&e))?;
    std::fs::rename(&tmp, &path).map_err(|e| err(&e))
}

/// Calls `on_change` once per burst of file events (create, write, rename-over, delete).
/// The returned monitor must be kept alive.
pub fn watch(on_change: impl Fn() + 'static) -> Result<gio::FileMonitor, glib::Error> {
    let path = path();
    if let Some(dir) = path.parent() {
        // inotify cannot report a file appearing in a directory that does not exist yet.
        let _ = std::fs::create_dir_all(dir);
    }
    let monitor = gio::File::for_path(&path)
        .monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)?;
    let on_change = Rc::new(on_change);
    let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
    monitor.connect_changed(move |_, _, _, event| {
        use gio::FileMonitorEvent as E;
        if !matches!(
            event,
            E::Changed | E::ChangesDoneHint | E::Created | E::Deleted | E::MovedIn | E::Renamed
        ) {
            return;
        }
        if let Some(id) = pending.borrow_mut().take() {
            id.remove();
        }
        let on_change = on_change.clone();
        let pending_inner = pending.clone();
        let id = glib::timeout_add_local_once(Duration::from_millis(150), move || {
            pending_inner.borrow_mut().take();
            on_change();
        });
        *pending.borrow_mut() = Some(id);
    });
    Ok(monitor)
}
