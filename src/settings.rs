//! The settings window. Every change is written to config.toml at once; the
//! daemon picks it up through its file monitor.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};

use crate::animations;
use crate::config::{self, Config, APP_ID, DAEMON_APP_ID, MAX_IDLE_MINUTES, MIN_IDLE_MINUTES};
use crate::dbus::{self, IdleMonitor, SessionManager};
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

struct Ui {
    app: adw::Application,
    window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    config: RefCell<Config>,
    bus: Option<gio::DBusConnection>,
    daemon_row: adw::ActionRow,
    start_button: gtk::Button,
    inhibitor_row: adw::ActionRow,
    preview: RefCell<Option<Stage>>,
    subscriptions: RefCell<Vec<gio::SignalSubscription>>,
}

impl Ui {
    fn build(app: &adw::Application) -> Rc<Ui> {
        let config = config::load().unwrap_or_else(|e| {
            info!("config: {e}; showing defaults, the next change overwrites the file");
            Config::default()
        });

        let enabled_row = adw::SwitchRow::builder()
            .title("Ekran koruyucu")
            .subtitle("Bilgisayar boşta kalınca tam ekran animasyon göster")
            .active(config.enabled)
            .build();
        let delay_row = adw::SpinRow::builder()
            .title("Boşta kalma süresi")
            .subtitle("Dakika")
            .adjustment(&gtk::Adjustment::new(
                f64::from(config.idle_minutes),
                f64::from(MIN_IDLE_MINUTES),
                f64::from(MAX_IDLE_MINUTES),
                1.0,
                5.0,
                0.0,
            ))
            .build();
        let inhibit_row = adw::SwitchRow::builder()
            .title("Video oynarken başlama")
            .subtitle("Bir uygulama boşta kalmayı engelliyorsa (video, sunum, görüntülü görüşme) ekran koruyucu açılmaz")
            .active(config.respect_inhibitors)
            .build();
        for row in [delay_row.upcast_ref::<gtk::Widget>(), inhibit_row.upcast_ref()] {
            enabled_row.bind_property("active", row, "sensitive").sync_create().build();
        }
        let general = adw::PreferencesGroup::new();
        general.add(&enabled_row);
        general.add(&delay_row);
        general.add(&inhibit_row);

        let titles: Vec<&str> = animations::ALL.iter().map(|a| a.title).collect();
        let selected = animations::ALL.iter().position(|a| a.id == config.animation).unwrap_or(0);
        let animation_row = adw::ComboRow::builder()
            .title("Animasyon")
            .model(&gtk::StringList::new(&titles))
            .selected(selected as u32)
            .build();
        let preview_button = gtk::Button::builder()
            .label("Önizle")
            .valign(gtk::Align::Center)
            .css_classes(["suggested-action"])
            .build();
        let preview_row = adw::ActionRow::builder()
            .title("Önizleme")
            .subtitle("Fareyi belirgin biçimde oynatın ya da bir tuşa basın; önizleme kapanır")
            .activatable_widget(&preview_button)
            .build();
        preview_row.add_suffix(&preview_button);
        let animation_group = adw::PreferencesGroup::builder().title("Animasyon").build();
        animation_group.add(&animation_row);
        animation_group.add(&preview_row);

        let daemon_row = adw::ActionRow::builder()
            .title("İzleyici")
            .subtitle("Denetleniyor…")
            .use_markup(false)
            .build();
        let start_button = gtk::Button::builder()
            .label("Başlat")
            .valign(gtk::Align::Center)
            .visible(false)
            .build();
        daemon_row.add_suffix(&start_button);
        let inhibitor_row = adw::ActionRow::builder()
            .title("Şu an boşta kalmayı engelleyen")
            .subtitle("Denetleniyor…")
            .subtitle_lines(0)
            .use_markup(false)
            .build();
        let status_group = adw::PreferencesGroup::builder().title("Durum").build();
        status_group.add(&daemon_row);
        status_group.add(&inhibitor_row);

        let page = adw::PreferencesPage::new();
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
            .title("Ekran Koruyucu")
            .default_width(560)
            .default_height(700)
            .content(&toolbar)
            .build();

        let ui = Rc::new(Ui {
            app: app.clone(),
            window,
            toasts,
            config: RefCell::new(config),
            bus: dbus::session_bus().map_err(|e| info!("session bus: {e}")).ok(),
            daemon_row,
            start_button: start_button.clone(),
            inhibitor_row,
            preview: RefCell::default(),
            subscriptions: RefCell::default(),
        });

        // Connected after the initial values are set, so opening the window writes nothing.
        let weak = Rc::downgrade(&ui);
        enabled_row.connect_active_notify(move |row| {
            let on = row.is_active();
            with(&weak, |ui| ui.update(|c| c.enabled = on));
        });
        let weak = Rc::downgrade(&ui);
        delay_row.connect_value_notify(move |row| {
            let minutes = row.value().round() as u32;
            with(&weak, |ui| ui.update(|c| c.idle_minutes = minutes));
        });
        let weak = Rc::downgrade(&ui);
        inhibit_row.connect_active_notify(move |row| {
            let on = row.is_active();
            with(&weak, |ui| ui.update(|c| c.respect_inhibitors = on));
        });
        let weak = Rc::downgrade(&ui);
        animation_row.connect_selected_notify(move |row| {
            if let Some(info) = animations::ALL.get(row.selected() as usize) {
                with(&weak, |ui| ui.update(|c| c.animation = info.id.to_owned()));
            }
        });
        let weak = Rc::downgrade(&ui);
        preview_button.connect_clicked(move |_| with(&weak, |ui| ui.show_preview()));
        let weak = Rc::downgrade(&ui);
        start_button.connect_clicked(move |_| with(&weak, |ui| ui.start_daemon()));

        ui.watch_daemon();
        ui.watch_inhibitors();
        ui
    }

    fn update(&self, change: impl FnOnce(&mut Config)) {
        let mut config = self.config.borrow_mut();
        let before = config.clone();
        change(&mut config);
        if *config == before {
            return;
        }
        if let Err(e) = config::save(&config) {
            info!("cannot save settings: {e}");
            self.toasts.add_toast(adw::Toast::new("Ayarlar kaydedilemedi"));
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
        self.daemon_row.set_subtitle(if running {
            "Çalışıyor"
        } else {
            "Çalışmıyor: ekran koruyucu devreye girmez. Oturum açılınca kendiliğinden başlar."
        });
        self.start_button.set_visible(!running);
    }

    fn start_daemon(&self) {
        let Ok(exe) = std::env::current_exe() else { return };
        let argv = [exe.as_os_str(), std::ffi::OsStr::new("daemon")];
        if let Err(e) = gio::Subprocess::newv(&argv, gio::SubprocessFlags::NONE) {
            info!("cannot start daemon: {e}");
            self.toasts.add_toast(adw::Toast::new("İzleyici başlatılamadı"));
        }
    }

    fn watch_inhibitors(self: &Rc<Self>) {
        let Some(bus) = &self.bus else {
            self.inhibitor_row.set_subtitle("Oturum veri yoluna ulaşılamadı");
            return;
        };
        let weak = Rc::downgrade(self);
        let subs = SessionManager::new(bus).subscribe_changes(move || with(&weak, |ui| ui.refresh_inhibitors()));
        *self.subscriptions.borrow_mut() = subs;
        self.refresh_inhibitors();
    }

    fn refresh_inhibitors(&self) {
        let Some(bus) = &self.bus else { return };
        let text = match SessionManager::new(bus).idle_inhibitors() {
            Ok(list) if list.is_empty() => "Yok".to_owned(),
            Ok(list) => {
                let mut lines = Vec::new();
                for inhibitor in &list {
                    lines.push(if inhibitor.is_wayland_surface() {
                        "Görünür bir uygulama penceresi (Wayland boşta-kalma isteği). \
                         GNOME hangi uygulama olduğunu D-Bus'ta bildirmiyor."
                            .to_owned()
                    } else if inhibitor.reason.is_empty() {
                        inhibitor.app_id.clone()
                    } else {
                        format!("{}: {}", inhibitor.app_id, inhibitor.reason)
                    });
                }
                if list.iter().any(|i| i.is_wayland_surface()) {
                    let players = dbus::playing_media_players(bus);
                    if !players.is_empty() {
                        lines.push(format!("Olası kaynak, şu an medya çalan: {}", players.join(", ")));
                    }
                }
                lines.join("\n")
            }
            Err(e) => format!("Okunamadı: {}", e.message()),
        };
        self.inhibitor_row.set_subtitle(&text);
    }
}

fn with(weak: &std::rc::Weak<Ui>, f: impl FnOnce(&Rc<Ui>)) {
    if let Some(ui) = weak.upgrade() {
        f(&ui);
    }
}
