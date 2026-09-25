use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use gtk::{gio, glib};

use crate::config::{self, Config, APP_ID};
use crate::dbus::{self, IdleMonitor};
use crate::stage::{Mode, Stage};

/// `preview`: the selected animation until input, or for `seconds`.
pub fn run(seconds: Option<u32>) -> glib::ExitCode {
    // NON_UNIQUE: runs beside an open settings window that owns APP_ID on the bus,
    // while windows keep APP_ID as their Wayland app_id.
    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let stage: Rc<RefCell<Option<Stage>>> = Rc::default();
    app.connect_activate(move |app| {
        let config = config::load().unwrap_or_else(|e| {
            info!("config: {e}; using defaults");
            Config::default()
        });
        let idle = dbus::session_bus().ok().map(|c| IdleMonitor::new(&c));
        let quit = app.clone();
        let opened = Stage::open(Some(app), &config, Mode::Preview, idle, move |_| quit.quit());
        if let Some(s) = seconds {
            opened.close_after(Duration::from_secs(s.into()));
        }
        *stage.borrow_mut() = Some(opened);
    });
    let argv0 = std::env::args().next().unwrap_or_default();
    app.run_with_args(&[argv0])
}
