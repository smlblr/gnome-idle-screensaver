//! The session daemon: turns Mutter idle watches and session inhibitors into
//! "show the screensaver now", and follows config.toml without restarting.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk::gio::prelude::*;
use gtk::{gio, glib};

use crate::config::{self, Config, APP_ID, DAEMON_APP_ID};
use crate::dbus::{self, IdleMonitor, SessionManager};
use crate::stage::{Mode, Stage};

pub fn run() -> glib::ExitCode {
    // SAFETY: still single-threaded; nothing has read the environment yet.
    unsafe {
        // GTK would spend a token inherited from the autostart launch on the first
        // window it maps, minutes later, when it can only yield an "is ready" notice.
        std::env::remove_var("XDG_ACTIVATION_TOKEN");
        std::env::remove_var("DESKTOP_STARTUP_ID");
    }
    // The Wayland app_id; it ties the screensaver window to the settings .desktop entry.
    glib::set_prgname(Some(APP_ID));

    let app = gio::Application::new(Some(DAEMON_APP_ID), gio::ApplicationFlags::empty());
    let daemon: Rc<RefCell<Option<Rc<Daemon>>>> = Rc::default();
    let slot = daemon.clone();
    app.connect_startup(move |app| match Daemon::start(app) {
        Ok(d) => *slot.borrow_mut() = Some(d),
        Err(e) => {
            info!("cannot start: {e}");
            app.quit();
        }
    });
    app.connect_activate(|_| debug!("activated by a second launch; already running"));

    if let Err(e) = app.register(gio::Cancellable::NONE) {
        info!("cannot register {DAEMON_APP_ID} on the session bus: {e}");
        return glib::ExitCode::FAILURE;
    }
    if app.is_remote() {
        info!("already running; nothing to do");
        app.activate();
        return glib::ExitCode::SUCCESS;
    }
    let argv0 = std::env::args().next().unwrap_or_default();
    let code = app.run_with_args(&[argv0]);
    drop(daemon);
    code
}

struct Daemon {
    _hold: gio::ApplicationHoldGuard,
    idle: IdleMonitor,
    session: SessionManager,
    config: RefCell<Config>,
    idle_watch: Cell<Option<u32>>,
    /// Only used while inhibited with `respect_inhibitors = false`: Mutter
    /// suppresses idle watches then, but GetIdletime keeps counting.
    deadline: RefCell<Option<glib::SourceId>>,
    stage: RefCell<Option<Stage>>,
    subscriptions: RefCell<Vec<gio::SignalSubscription>>,
    file_monitor: RefCell<Option<gio::FileMonitor>>,
}

impl Daemon {
    fn start(app: &gio::Application) -> Result<Rc<Daemon>, String> {
        let conn = dbus::session_bus().map_err(|e| e.to_string())?;
        let config = config::load().unwrap_or_else(|e| {
            info!("config: {e}; using defaults");
            Config::default()
        });
        let daemon = Rc::new(Daemon {
            _hold: app.hold(),
            idle: IdleMonitor::new(&conn),
            session: SessionManager::new(&conn),
            config: RefCell::new(config),
            idle_watch: Cell::new(None),
            deadline: RefCell::default(),
            stage: RefCell::default(),
            subscriptions: RefCell::default(),
            file_monitor: RefCell::default(),
        });

        let weak = Rc::downgrade(&daemon);
        let watch_fired = daemon.idle.subscribe_watch_fired(move |id| {
            if let Some(d) = weak.upgrade() {
                d.on_watch_fired(id);
            }
        });
        let weak = Rc::downgrade(&daemon);
        let inhibitors = daemon.session.subscribe_changes(move || {
            if let Some(d) = weak.upgrade() {
                d.update_deadline();
            }
        });
        daemon.subscriptions.borrow_mut().push(watch_fired);
        daemon.subscriptions.borrow_mut().extend(inhibitors);

        let weak = Rc::downgrade(&daemon);
        match config::watch(move || {
            if let Some(d) = weak.upgrade() {
                d.reload();
            }
        }) {
            Ok(monitor) => *daemon.file_monitor.borrow_mut() = Some(monitor),
            Err(e) => info!("cannot watch {}: {e}; changes need a restart", config::path().display()),
        }

        // Watches belong to our D-Bus connection inside Mutter, so (re)create them
        // whenever the service gains an owner, including right now if it has one.
        let (weak_a, weak_v) = (Rc::downgrade(&daemon), Rc::downgrade(&daemon));
        // Never unwatched: it lives as long as the daemon.
        gio::bus_watch_name_on_connection(
            &conn,
            "org.gnome.Mutter.IdleMonitor",
            gio::BusNameWatcherFlags::NONE,
            move |_, _, _| {
                if let Some(d) = weak_a.upgrade() {
                    d.rearm();
                }
            },
            move |_, _| {
                if let Some(d) = weak_v.upgrade() {
                    info!("Mutter IdleMonitor vanished; waiting for it to return");
                    d.idle_watch.set(None);
                }
            },
        );

        // For tests and scripts: `show` with a uint32 auto-close in seconds (0 = until input).
        let show = gio::SimpleAction::new("show", Some(glib::VariantTy::UINT32));
        let weak = Rc::downgrade(&daemon);
        show.connect_activate(move |_, param| {
            let seconds = param.and_then(|p| p.get::<u32>()).unwrap_or(0);
            if let Some(d) = weak.upgrade() {
                let close_after = (seconds > 0).then(|| Duration::from_secs(seconds.into()));
                d.show("show action", close_after);
            }
        });
        app.add_action(&show);

        info!("started: {} ({})", describe(&daemon.config.borrow()), config::path().display());
        Ok(daemon)
    }

    fn rearm(self: &Rc<Self>) {
        if let Some(id) = self.idle_watch.take() {
            self.idle.remove_watch(id);
        }
        let cfg = self.config.borrow().clone();
        if cfg.enabled {
            match self.idle.add_idle_watch(cfg.idle_ms()) {
                Ok(id) => {
                    self.idle_watch.set(Some(id));
                    debug!("idle watch {id} armed for {} ms", cfg.idle_ms());
                }
                Err(e) => info!("AddIdleWatch failed: {e}"),
            }
        }
        self.update_deadline();
    }

    fn on_watch_fired(self: &Rc<Self>, id: u32) {
        if self.idle_watch.get() != Some(id) {
            return;
        }
        let cfg = self.config.borrow().clone();
        if !cfg.enabled || self.stage.borrow().is_some() {
            return;
        }
        // Mutter already holds idle watches back while inhibited; this covers an
        // inhibitor that appeared in the same instant.
        if cfg.respect_inhibitors && self.session.is_idle_inhibited().unwrap_or(false) {
            info!("idle timeout reached but an idle inhibitor is active; not starting");
            return;
        }
        self.show("idle timeout", None);
    }

    /// With `respect_inhibitors = false`, schedules the show for when the user will
    /// have been idle long enough; re-evaluated on every inhibitor or config change.
    fn update_deadline(self: &Rc<Self>) {
        if let Some(id) = self.deadline.borrow_mut().take() {
            id.remove();
        }
        let cfg = self.config.borrow().clone();
        if !cfg.enabled || cfg.respect_inhibitors || self.stage.borrow().is_some() {
            return;
        }
        if !self.session.is_idle_inhibited().unwrap_or(false) {
            return;
        }
        let idle = match self.idle.idletime_ms() {
            Ok(ms) => ms,
            Err(e) => {
                info!("GetIdletime failed: {e}");
                return;
            }
        };
        let remaining = cfg.idle_ms().saturating_sub(idle);
        if remaining < 100 {
            self.show("idle timeout, inhibitor ignored by setting", None);
            return;
        }
        debug!("idle inhibited and ignored by setting; checking again in {remaining} ms");
        let weak = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(Duration::from_millis(remaining), move || {
            if let Some(d) = weak.upgrade() {
                d.deadline.borrow_mut().take();
                d.update_deadline();
            }
        });
        *self.deadline.borrow_mut() = Some(id);
    }

    fn show(self: &Rc<Self>, why: &str, close_after: Option<Duration>) {
        if self.stage.borrow().is_some() {
            return;
        }
        if !gtk::is_initialized_main_thread() {
            // Deferred to the first show: a daemon that has never shown anything
            // stays a small D-Bus client.
            if let Err(e) = gtk::init() {
                info!("cannot initialise GTK, not showing: {e}");
                return;
            }
        }
        if let Some(id) = self.deadline.borrow_mut().take() {
            id.remove();
        }
        info!("showing screensaver ({why}); user idle for {} ms", self.idle.idletime_ms().unwrap_or(0));
        let weak = Rc::downgrade(self);
        let stage = Stage::open(None, &self.config.borrow(), Mode::Idle, Some(self.idle.clone()), move |_| {
            if let Some(d) = weak.upgrade() {
                d.stage.borrow_mut().take();
                d.update_deadline();
            }
        });
        if let Some(after) = close_after {
            stage.close_after(after);
        }
        *self.stage.borrow_mut() = Some(stage);
    }

    fn reload(self: &Rc<Self>) {
        match config::load() {
            Ok(new) if new == *self.config.borrow() => debug!("config file touched, no change"),
            Ok(new) => {
                info!("config reloaded: {}", describe(&new));
                *self.config.borrow_mut() = new;
                self.rearm();
            }
            Err(e) => info!("config reload failed, keeping previous settings: {e}"),
        }
    }
}

fn describe(c: &Config) -> String {
    format!(
        "enabled={} idle_minutes={} respect_inhibitors={} animation={}",
        c.enabled, c.idle_minutes, c.respect_inhibitors, c.animation
    )
}
