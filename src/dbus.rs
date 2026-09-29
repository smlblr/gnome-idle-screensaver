//! Thin synchronous wrappers over the two GNOME services the screensaver depends on.
//! Calls are rare (a watch firing, an inhibitor changing), so blocking for one
//! round trip on the main loop is cheaper than carrying async state around.

use gtk::gio::prelude::*;
use gtk::glib::variant::ObjectPath;
use gtk::{gio, glib};

const IDLE_BUS: &str = "org.gnome.Mutter.IdleMonitor";
const IDLE_PATH: &str = "/org/gnome/Mutter/IdleMonitor/Core";
const IDLE_IFACE: &str = "org.gnome.Mutter.IdleMonitor";

const SM_BUS: &str = "org.gnome.SessionManager";
const SM_PATH: &str = "/org/gnome/SessionManager";
const SM_IFACE: &str = "org.gnome.SessionManager";
const INHIBITOR_IFACE: &str = "org.gnome.SessionManager.Inhibitor";

/// GsmInhibitorFlag IDLE. Flag 4 (suspend) is deliberately ignored.
pub const INHIBIT_IDLE: u32 = 8;

const TIMEOUT_MS: i32 = 2000;

pub fn session_bus() -> Result<gio::DBusConnection, glib::Error> {
    gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
}

// Every argument is a distinct D-Bus call parameter; a struct would only rename them.
#[allow(clippy::too_many_arguments)]
fn call(
    conn: &gio::DBusConnection,
    dest: &str,
    path: &str,
    iface: &str,
    method: &str,
    args: Option<&glib::Variant>,
    reply: &str,
    timeout_ms: i32,
) -> Result<glib::Variant, glib::Error> {
    conn.call_sync(
        Some(dest),
        path,
        iface,
        method,
        args,
        Some(glib::VariantTy::new(reply).expect("valid reply type")),
        gio::DBusCallFlags::NONE,
        timeout_ms,
        gio::Cancellable::NONE,
    )
}

fn single<T: glib::variant::FromVariant>(v: glib::Variant) -> T {
    v.get::<(T,)>().expect("reply type was checked by GDBus").0
}

/// `org.gnome.Mutter.IdleMonitor` on the core (all devices) monitor.
#[derive(Clone)]
pub struct IdleMonitor {
    conn: gio::DBusConnection,
}

impl IdleMonitor {
    pub fn new(conn: &gio::DBusConnection) -> Self {
        Self { conn: conn.clone() }
    }

    fn call(
        &self,
        method: &str,
        args: Option<glib::Variant>,
        reply: &str,
    ) -> Result<glib::Variant, glib::Error> {
        call(
            &self.conn,
            IDLE_BUS,
            IDLE_PATH,
            IDLE_IFACE,
            method,
            args.as_ref(),
            reply,
            TIMEOUT_MS,
        )
    }

    pub fn idletime_ms(&self) -> Result<u64, glib::Error> {
        self.call("GetIdletime", None, "(t)").map(single)
    }

    /// Mutter keeps this watch across activity and fires it again after each idle
    /// period, but never while a session idle inhibitor exists.
    pub fn add_idle_watch(&self, interval_ms: u64) -> Result<u32, glib::Error> {
        self.call("AddIdleWatch", Some((interval_ms,).to_variant()), "(u)")
            .map(single)
    }

    /// One-shot: Mutter removes it after it fires.
    pub fn add_user_active_watch(&self) -> Result<u32, glib::Error> {
        self.call("AddUserActiveWatch", None, "(u)").map(single)
    }

    pub fn remove_watch(&self, id: u32) {
        if let Err(e) = self.call("RemoveWatch", Some((id,).to_variant()), "()") {
            debug!("RemoveWatch({id}) failed: {e}");
        }
    }

    pub fn subscribe_watch_fired(
        &self,
        on_fired: impl Fn(u32) + 'static,
    ) -> gio::SignalSubscription {
        self.conn.subscribe_to_signal(
            Some(IDLE_BUS),
            Some(IDLE_IFACE),
            Some("WatchFired"),
            Some(IDLE_PATH),
            None,
            gio::DBusSignalFlags::NONE,
            move |signal| {
                if let Some((id,)) = signal.parameters.get::<(u32,)>() {
                    on_fired(id);
                }
            },
        )
    }
}

#[derive(Debug, Clone)]
pub struct Inhibitor {
    pub app_id: String,
    pub reason: String,
}

impl Inhibitor {
    /// Mutter files one anonymous inhibitor for every Wayland surface that uses
    /// zwp_idle_inhibit; which window asked is not exposed over D-Bus.
    pub fn is_wayland_surface(&self) -> bool {
        self.app_id == "mutter" && self.reason == "idle-inhibit"
    }
}

/// `org.gnome.SessionManager`: inhibitor queries and change notifications.
#[derive(Clone)]
pub struct SessionManager {
    conn: gio::DBusConnection,
}

impl SessionManager {
    pub fn new(conn: &gio::DBusConnection) -> Self {
        Self { conn: conn.clone() }
    }

    pub fn is_idle_inhibited(&self) -> Result<bool, glib::Error> {
        call(
            &self.conn,
            SM_BUS,
            SM_PATH,
            SM_IFACE,
            "IsInhibited",
            Some(&(INHIBIT_IDLE,).to_variant()),
            "(b)",
            TIMEOUT_MS,
        )
        .map(single)
    }

    pub fn idle_inhibitors(&self) -> Result<Vec<Inhibitor>, glib::Error> {
        let paths: Vec<ObjectPath> = single(call(
            &self.conn,
            SM_BUS,
            SM_PATH,
            SM_IFACE,
            "GetInhibitors",
            None,
            "(ao)",
            TIMEOUT_MS,
        )?);
        let mut found = Vec::new();
        for path in paths {
            let get = |method: &str, reply: &str| {
                call(
                    &self.conn,
                    SM_BUS,
                    path.as_str(),
                    INHIBITOR_IFACE,
                    method,
                    None,
                    reply,
                    TIMEOUT_MS,
                )
            };
            // An inhibitor can disappear between GetInhibitors and these calls.
            let Ok(flags) = get("GetFlags", "(u)").map(single::<u32>) else {
                continue;
            };
            if flags & INHIBIT_IDLE == 0 {
                continue;
            }
            let app_id = get("GetAppId", "(s)")
                .map(single::<String>)
                .unwrap_or_default();
            let reason = get("GetReason", "(s)")
                .map(single::<String>)
                .unwrap_or_default();
            found.push(Inhibitor { app_id, reason });
        }
        Ok(found)
    }

    /// Fires on InhibitorAdded/InhibitorRemoved and on InhibitedActions changes.
    pub fn subscribe_changes(
        &self,
        on_change: impl Fn() + 'static,
    ) -> Vec<gio::SignalSubscription> {
        let on_change = std::rc::Rc::new(on_change);
        let sub = |iface: &str, member: &str| {
            let on_change = on_change.clone();
            self.conn.subscribe_to_signal(
                Some(SM_BUS),
                Some(iface),
                Some(member),
                Some(SM_PATH),
                None,
                gio::DBusSignalFlags::NONE,
                move |_| on_change(),
            )
        };
        vec![
            sub(SM_IFACE, "InhibitorAdded"),
            sub(SM_IFACE, "InhibitorRemoved"),
            sub("org.freedesktop.DBus.Properties", "PropertiesChanged"),
        ]
    }
}

/// Identities of MPRIS players reporting "Playing". Only a hint for the settings
/// window: it cannot prove which window holds Mutter's anonymous inhibitor.
pub fn playing_media_players(conn: &gio::DBusConnection) -> Vec<String> {
    const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";
    const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";
    let Ok(names) = call(
        conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "ListNames",
        None,
        "(as)",
        TIMEOUT_MS,
    )
    .map(single::<Vec<String>>) else {
        return Vec::new();
    };
    let prop = |name: &str, iface: &str, prop: &str| -> Option<String> {
        let v = call(
            conn,
            name,
            MPRIS_PATH,
            "org.freedesktop.DBus.Properties",
            "Get",
            Some(&(iface, prop).to_variant()),
            "(v)",
            500,
        )
        .ok()?;
        single::<glib::Variant>(v).get::<String>()
    };
    names
        .iter()
        .filter(|n| n.starts_with(MPRIS_PREFIX))
        .filter(|n| {
            prop(n, "org.mpris.MediaPlayer2.Player", "PlaybackStatus").as_deref() == Some("Playing")
        })
        .map(|n| {
            prop(n, "org.mpris.MediaPlayer2", "Identity")
                .unwrap_or_else(|| n[MPRIS_PREFIX.len()..].to_owned())
        })
        .collect()
}
