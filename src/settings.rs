//! The settings window. Every change is written to config.toml at once; the
//! daemon picks it up through its file monitor.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};

use crate::animations;
use crate::config::{self, APP_ID, Config, DAEMON_APP_ID, MAX_IDLE_MINUTES, MIN_IDLE_MINUTES};
use crate::dbus::{self, IdleMonitor, SessionManager};
use crate::i18n::{self, LANGUAGE_NAMES, LANGUAGE_TITLE, Lang, Msg};
use crate::stage::{Mode, Stage};

pub fn run() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    let ui: Rc<RefCell<Option<Rc<Ui>>>> = Rc::default();
    app.connect_activate(move |app| {
        if let Some(window) = app.active_window() {
            window.present();
            return;
        }
        let built = Ui::build(app);
        built.window.present();
        *ui.borrow_mut() = Some(built);
    });
    let argv0 = std::env::args().next().unwrap_or_default();
    app.run_with_args(&[argv0])
}

/// Everything whose text depends on the interface language.
struct Widgets {
    enabled: adw::SwitchRow,
    delay: adw::SpinRow,
    inhibit: adw::SwitchRow,
    language: adw::ComboRow,
    animation_group: adw::PreferencesGroup,
    animation: adw::ComboRow,
    preview_button: gtk::Button,
    preview_row: adw::ActionRow,
    status_group: adw::PreferencesGroup,
    daemon_row: adw::ActionRow,
    start_button: gtk::Button,
    inhibitor_row: adw::ActionRow,
}

struct Ui {
    app: adw::Application,
    window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    w: Widgets,
    config: RefCell<Config>,
    lang: Cell<Lang>,
    /// Rebuilding a combo model resets its selection and fires notify; nothing may be saved then.
    retranslating: Cell<bool>,
    bus: Option<gio::DBusConnection>,
    daemon_running: Cell<Option<bool>>,
    preview: RefCell<Option<Stage>>,
    subscriptions: RefCell<Vec<gio::SignalSubscription>>,
}

impl Ui {
    fn build(app: &adw::Application) -> Rc<Ui> {
        let config = config::load().unwrap_or_else(|e| {
            info!("config: {e}; showing defaults, the next change overwrites the file");
            Config::default()
        });
        let lang = Lang::resolve(&config.language);

        let enabled = adw::SwitchRow::builder().active(config.enabled).build();
        let delay = adw::SpinRow::builder()
            .adjustment(&gtk::Adjustment::new(
                f64::from(config.idle_minutes),
                f64::from(MIN_IDLE_MINUTES),
                f64::from(MAX_IDLE_MINUTES),
                1.0,
                5.0,
                0.0,
            ))
            .build();
        let inhibit = adw::SwitchRow::builder()
            .active(config.respect_inhibitors)
            .build();
        for row in [delay.upcast_ref::<gtk::Widget>(), inhibit.upcast_ref()] {
            enabled
                .bind_property("active", row, "sensitive")
                .sync_create()
                .build();
        }
        let general = adw::PreferencesGroup::new();
        general.add(&enabled);
        general.add(&delay);
        general.add(&inhibit);

        let language = adw::ComboRow::builder()
            .title(LANGUAGE_TITLE)
            .model(&gtk::StringList::new(&LANGUAGE_NAMES))
            .selected(
                i18n::SETTINGS
                    .iter()
                    .position(|s| *s == config.language)
                    .unwrap_or(0) as u32,
            )
            .build();
        let language_group = adw::PreferencesGroup::new();
        language_group.add(&language);

        let animation = adw::ComboRow::new();
        let preview_button = gtk::Button::builder()
            .valign(gtk::Align::Center)
            .css_classes(["suggested-action"])
            .build();
        let preview_row = adw::ActionRow::builder()
            .activatable_widget(&preview_button)
            .build();
        preview_row.add_suffix(&preview_button);
        let animation_group = adw::PreferencesGroup::new();
        animation_group.add(&animation);
        animation_group.add(&preview_row);

        let daemon_row = adw::ActionRow::builder().use_markup(false).build();
        let start_button = gtk::Button::builder()
            .valign(gtk::Align::Center)
            .visible(false)
            .build();
        daemon_row.add_suffix(&start_button);
        let inhibitor_row = adw::ActionRow::builder()
            .subtitle_lines(0)
            .use_markup(false)
            .build();
        let status_group = adw::PreferencesGroup::new();
        status_group.add(&daemon_row);
        status_group.add(&inhibitor_row);

        let page = adw::PreferencesPage::new();
        page.add(&language_group);
        page.add(&general);
        page.add(&animation_group);
        page.add(&status_group);
        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&page));
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        toolbar.set_content(Some(&toasts));
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .default_width(560)
            .default_height(760)
            .content(&toolbar)
            .build();

        let ui = Rc::new(Ui {
            app: app.clone(),
            window,
            toasts,
            w: Widgets {
                enabled,
                delay,
                inhibit,
                language,
                animation_group,
                animation,
                preview_button,
                preview_row,
                status_group,
                daemon_row,
                start_button,
                inhibitor_row,
            },
            config: RefCell::new(config),
            lang: Cell::new(lang),
            retranslating: Cell::new(false),
            bus: dbus::session_bus()
                .map_err(|e| info!("session bus: {e}"))
                .ok(),
            daemon_running: Cell::new(None),
            preview: RefCell::default(),
            subscriptions: RefCell::default(),
        });
        ui.retranslate();

        // Connected after the initial values are set, so opening the window writes nothing.
        let weak = Rc::downgrade(&ui);
        ui.w.enabled.connect_active_notify(move |row| {
            let on = row.is_active();
            with(&weak, |ui| ui.update(|c| c.enabled = on));
        });
        let weak = Rc::downgrade(&ui);
        ui.w.delay.connect_value_notify(move |row| {
            let minutes = row.value().round() as u32;
            with(&weak, |ui| ui.update(|c| c.idle_minutes = minutes));
        });
        let weak = Rc::downgrade(&ui);
        ui.w.inhibit.connect_active_notify(move |row| {
            let on = row.is_active();
            with(&weak, |ui| ui.update(|c| c.respect_inhibitors = on));
        });
        let weak = Rc::downgrade(&ui);
        ui.w.animation.connect_selected_notify(move |row| {
            if let Some(info) = animations::ALL.get(row.selected() as usize) {
                with(&weak, |ui| ui.update(|c| c.animation = info.id.to_owned()));
            }
        });
        let weak = Rc::downgrade(&ui);
        ui.w.language.connect_selected_notify(move |row| {
            let Some(&setting) = i18n::SETTINGS.get(row.selected() as usize) else {
                return;
            };
            with(&weak, |ui| {
                if ui.config.borrow().language == setting {
                    return;
                }
                ui.update(|c| c.language = setting.to_owned());
                ui.lang.set(Lang::resolve(setting));
                // Not from inside this handler: GObject holds back notifications raised
                // while it runs, and they would arrive after the guard is released.
                let weak = Rc::downgrade(ui);
                glib::idle_add_local_once(move || with(&weak, |ui| ui.retranslate()));
            });
        });
        let weak = Rc::downgrade(&ui);
        ui.w.preview_button
            .connect_clicked(move |_| with(&weak, |ui| ui.show_preview()));
        let weak = Rc::downgrade(&ui);
        ui.w.start_button
            .connect_clicked(move |_| with(&weak, |ui| ui.start_daemon()));

        ui.watch_daemon();
        ui.watch_inhibitors();
        ui
    }

    fn text(&self, msg: Msg) -> &'static str {
        msg.text(self.lang.get())
    }

    /// Sets every language-dependent string; safe to call again when the language changes.
    fn retranslate(&self) {
        self.retranslating.set(true);
        let lang = self.lang.get();
        let t = |msg: Msg| msg.text(lang);
        let w = &self.w;

        self.window.set_title(Some(t(Msg::WindowTitle)));
        w.enabled.set_title(t(Msg::EnabledTitle));
        w.enabled.set_subtitle(t(Msg::EnabledSubtitle));
        w.delay.set_title(t(Msg::DelayTitle));
        w.delay.set_subtitle(t(Msg::DelaySubtitle));
        w.inhibit.set_title(t(Msg::InhibitTitle));
        w.inhibit.set_subtitle(t(Msg::InhibitSubtitle));
        w.animation_group.set_title(t(Msg::AnimationGroup));
        w.animation.set_title(t(Msg::AnimationTitle));
        w.preview_button.set_label(t(Msg::PreviewButton));
        w.preview_row.set_title(t(Msg::PreviewTitle));
        w.preview_row.set_subtitle(t(Msg::PreviewSubtitle));
        w.status_group.set_title(t(Msg::StatusGroup));
        w.daemon_row.set_title(t(Msg::DaemonTitle));
        w.start_button.set_label(t(Msg::StartButton));
        w.inhibitor_row.set_title(t(Msg::InhibitorsTitle));
        w.language.set_subtitle(t(Msg::LanguageSubtitle));

        let config = self.config.borrow();
        let titles: Vec<&str> = animations::ALL.iter().map(|a| a.title(lang)).collect();
        w.animation.set_model(Some(&gtk::StringList::new(&titles)));
        w.animation.set_selected(
            animations::ALL
                .iter()
                .position(|a| a.id == config.animation)
                .unwrap_or(0) as u32,
        );
        drop(config);
        self.retranslating.set(false);

        self.show_daemon_state();
        self.refresh_inhibitors();
    }

    fn update(&self, change: impl FnOnce(&mut Config)) {
        if self.retranslating.get() {
            return;
        }
        let mut config = self.config.borrow_mut();
        let before = config.clone();
        change(&mut config);
        if *config == before {
            return;
        }
        if let Err(e) = config::save(&config) {
            info!("cannot save settings: {e}");
            self.toasts
                .add_toast(adw::Toast::new(self.text(Msg::SaveFailed)));
        }
    }

    fn show_preview(self: &Rc<Self>) {
        if self.preview.borrow().is_some() {
            return;
        }
        let weak = Rc::downgrade(self);
        let idle = self.bus.as_ref().map(IdleMonitor::new);
        let stage = Stage::open(
            Some(self.app.upcast_ref()),
            &self.config.borrow(),
            Mode::Preview,
            idle,
            move |_| with(&weak, |ui| drop(ui.preview.borrow_mut().take())),
        );
        *self.preview.borrow_mut() = Some(stage);
    }

    fn watch_daemon(self: &Rc<Self>) {
        let (weak_a, weak_v) = (Rc::downgrade(self), Rc::downgrade(self));
        gio::bus_watch_name(
            gio::BusType::Session,
            DAEMON_APP_ID,
            gio::BusNameWatcherFlags::NONE,
            move |_, _, _| with(&weak_a, |ui| ui.set_daemon_running(true)),
            move |_, _| with(&weak_v, |ui| ui.set_daemon_running(false)),
        );
    }

    fn set_daemon_running(&self, running: bool) {
        self.daemon_running.set(Some(running));
        self.show_daemon_state();
    }

    fn show_daemon_state(&self) {
        let state = self.daemon_running.get();
        self.w.daemon_row.set_subtitle(match state {
            None => self.text(Msg::Checking),
            Some(true) => self.text(Msg::DaemonRunning),
            Some(false) => self.text(Msg::DaemonStopped),
        });
        self.w.start_button.set_visible(state == Some(false));
    }

    fn start_daemon(&self) {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let argv = [exe.as_os_str(), std::ffi::OsStr::new("daemon")];
        if let Err(e) = gio::Subprocess::newv(&argv, gio::SubprocessFlags::NONE) {
            info!("cannot start daemon: {e}");
            self.toasts
                .add_toast(adw::Toast::new(self.text(Msg::StartFailed)));
        }
    }

    fn watch_inhibitors(self: &Rc<Self>) {
        if let Some(bus) = &self.bus {
            let weak = Rc::downgrade(self);
            let subs = SessionManager::new(bus)
                .subscribe_changes(move || with(&weak, |ui| ui.refresh_inhibitors()));
            *self.subscriptions.borrow_mut() = subs;
        }
        self.refresh_inhibitors();
    }

    fn refresh_inhibitors(&self) {
        let Some(bus) = &self.bus else {
            self.w.inhibitor_row.set_subtitle(self.text(Msg::NoBus));
            return;
        };
        let text = match SessionManager::new(bus).idle_inhibitors() {
            Ok(list) if list.is_empty() => self.text(Msg::NoInhibitors).to_owned(),
            Ok(list) => {
                let mut lines = Vec::new();
                for inhibitor in &list {
                    lines.push(if inhibitor.is_wayland_surface() {
                        self.text(Msg::WaylandSurface).to_owned()
                    } else if inhibitor.reason.is_empty() {
                        inhibitor.app_id.clone()
                    } else {
                        format!("{}: {}", inhibitor.app_id, inhibitor.reason)
                    });
                }
                if list.iter().any(|i| i.is_wayland_surface()) {
                    let players = dbus::playing_media_players(bus);
                    if !players.is_empty() {
                        lines.push(format!(
                            "{}{}",
                            self.text(Msg::LikelySource),
                            players.join(", ")
                        ));
                    }
                }
                lines.join("\n")
            }
            Err(e) => format!("{}{}", self.text(Msg::CannotRead), e.message()),
        };
        self.w.inhibitor_row.set_subtitle(&text);
    }
}

fn with(weak: &std::rc::Weak<Ui>, f: impl FnOnce(&Rc<Ui>)) {
    if let Some(ui) = weak.upgrade() {
        f(&ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pump() {
        while glib::MainContext::default().iteration(false) {}
    }

    /// Builds the real window against a temporary config and drives the language row
    /// the way a click would. GTK starts once per process: run via tools/gui-tests.sh.
    #[test]
    #[ignore = "needs a graphical session"]
    fn switching_language_retranslates_and_saves_only_the_language() {
        adw::init().expect("graphical session");
        let dir = std::env::temp_dir().join(format!("gis-settings-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "animation = \"aurora\"\nidle_minutes = 7\nlanguage = \"en\"\n",
        )
        .unwrap();
        // SAFETY: the test runs single-threaded (--test-threads=1) and nothing else reads the variable.
        unsafe { std::env::set_var("GNOME_IDLE_SCREENSAVER_CONFIG", &path) };

        let app = adw::Application::builder()
            .application_id(APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(gio::Cancellable::NONE).unwrap();
        let ui = Ui::build(&app);
        let aurora = animations::ALL
            .iter()
            .position(|a| a.id == "aurora")
            .unwrap() as u32;

        assert_eq!(ui.w.enabled.title().as_str(), "Screensaver");
        assert_eq!(ui.w.animation.selected(), aurora);
        assert_eq!(
            config::load().unwrap().language,
            "en",
            "opening the window must not write"
        );

        ui.w.language.set_selected(2);
        pump();
        assert_eq!(ui.w.enabled.title().as_str(), "Ekran koruyucu");
        assert_eq!(ui.w.preview_button.label().unwrap().as_str(), "Önizle");
        assert_eq!(ui.window.title().unwrap().as_str(), "Ekran Koruyucu");
        assert_eq!(
            ui.w.language.selected(),
            2,
            "the language row keeps its own selection"
        );
        assert_eq!(
            ui.w.animation.selected(),
            aurora,
            "rebuilding the model must keep the animation"
        );
        let saved = config::load().unwrap();
        assert_eq!(saved.language, "tr");
        assert_eq!(
            saved.animation, "aurora",
            "retranslating must not overwrite other settings"
        );
        assert_eq!(saved.idle_minutes, 7);

        ui.w.language.set_selected(1);
        pump();
        assert_eq!(ui.w.enabled.title().as_str(), "Screensaver");
        assert_eq!(config::load().unwrap().language, "en");

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
