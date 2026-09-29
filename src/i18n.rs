//! User-interface language. Two languages are compiled in; `Msg::text` is an exhaustive
//! match, so a message missing one of them does not compile. Log lines stay English.
//! docs/architecture.md, "Adding a language", explains why this is not gettext.

use gtk::glib;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Tr,
}

/// Values of `language` in config.toml, in the order the settings window lists them.
pub const SETTINGS: [&str; 3] = ["auto", "en", "tr"];

impl Lang {
    /// `setting` is the config value: "auto" follows the desktop's language, anything
    /// unknown does the same rather than failing.
    pub fn resolve(setting: &str) -> Lang {
        match setting {
            "en" => Lang::En,
            "tr" => Lang::Tr,
            _ => Lang::system(),
        }
    }

    pub fn system() -> Lang {
        Lang::from_locales(glib::language_names().iter().map(|n| n.as_str()))
    }

    /// The first locale that names a supported language wins; "C" and others fall through.
    fn from_locales<'a>(locales: impl IntoIterator<Item = &'a str>) -> Lang {
        for locale in locales {
            let tag = locale.to_ascii_lowercase();
            if tag == "tr"
                || tag.starts_with("tr_")
                || tag.starts_with("tr-")
                || tag.starts_with("tr.")
            {
                return Lang::Tr;
            }
            if tag == "en"
                || tag.starts_with("en_")
                || tag.starts_with("en-")
                || tag.starts_with("en.")
            {
                return Lang::En;
            }
        }
        Lang::En
    }

    pub fn pick(self, en: &'static str, tr: &'static str) -> &'static str {
        match self {
            Lang::En => en,
            Lang::Tr => tr,
        }
    }
}

macro_rules! messages {
    ($($name:ident => ($en:expr, $tr:expr)),* $(,)?) => {
        #[derive(Clone, Copy, Debug)]
        pub enum Msg { $($name),* }

        impl Msg {
            #[cfg(test)]
            pub const ALL: &'static [Msg] = &[$(Msg::$name),*];

            pub fn text(self, lang: Lang) -> &'static str {
                match self { $(Msg::$name => lang.pick($en, $tr)),* }
            }
        }
    };
}

messages! {
    WindowTitle => ("Screensaver", "Ekran Koruyucu"),
    EnabledTitle => ("Screensaver", "Ekran koruyucu"),
    EnabledSubtitle => (
        "Show a full-screen animation when the computer is idle",
        "Bilgisayar boşta kalınca tam ekran animasyon göster"
    ),
    DelayTitle => ("Idle time before starting", "Boşta kalma süresi"),
    DelaySubtitle => ("Minutes", "Dakika"),
    InhibitTitle => ("Don't start while video is playing", "Video oynarken başlama"),
    InhibitSubtitle => (
        "If an app is preventing idle (video, presentation, video call), the screensaver does not start",
        "Bir uygulama boşta kalmayı engelliyorsa (video, sunum, görüntülü görüşme) ekran koruyucu açılmaz"
    ),
    AnimationGroup => ("Animation", "Animasyon"),
    AnimationTitle => ("Animation", "Animasyon"),
    PreviewButton => ("Preview", "Önizle"),
    PreviewTitle => ("Preview", "Önizleme"),
    PreviewSubtitle => (
        "Move the mouse noticeably or press a key to close the preview",
        "Fareyi belirgin biçimde oynatın ya da bir tuşa basın; önizleme kapanır"
    ),
    StatusGroup => ("Status", "Durum"),
    DaemonTitle => ("Watcher", "İzleyici"),
    Checking => ("Checking…", "Denetleniyor…"),
    DaemonRunning => ("Running", "Çalışıyor"),
    DaemonStopped => (
        "Not running: the screensaver will not start. It starts automatically at login.",
        "Çalışmıyor: ekran koruyucu devreye girmez. Oturum açılınca kendiliğinden başlar."
    ),
    StartButton => ("Start", "Başlat"),
    InhibitorsTitle => ("Currently preventing idle", "Şu an boşta kalmayı engelleyen"),
    NoInhibitors => ("None", "Yok"),
    WaylandSurface => (
        "A visible application window (Wayland idle request). GNOME does not say which app over D-Bus.",
        "Görünür bir uygulama penceresi (Wayland boşta-kalma isteği). GNOME hangi uygulama olduğunu D-Bus'ta bildirmiyor."
    ),
    LikelySource => ("Possible source, currently playing media: ", "Olası kaynak, şu an medya çalan: "),
    CannotRead => ("Could not read: ", "Okunamadı: "),
    NoBus => ("Could not reach the session bus", "Oturum veri yoluna ulaşılamadı"),
    SaveFailed => ("Could not save settings", "Ayarlar kaydedilemedi"),
    StartFailed => ("Could not start the watcher", "İzleyici başlatılamadı"),
    LanguageSubtitle => (
        "Language of this window",
        "Bu pencerenin dili"
    ),
}

/// Shown identically in every language so that someone who cannot read the current one
/// can still find the switch.
pub const LANGUAGE_TITLE: &str = "Language / Dil";
/// Fixed for the same reason, and because the language row's model must never be rebuilt
/// while it is being used; see docs/architecture.md, "Language switching".
pub const LANGUAGE_NAMES: [&str; 3] = ["System default / Sistem varsayılanı", "English", "Türkçe"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_message_has_text_in_both_languages() {
        for &msg in Msg::ALL {
            for lang in [Lang::En, Lang::Tr] {
                assert!(
                    !msg.text(lang).trim().is_empty(),
                    "{msg:?} is empty in {lang:?}"
                );
            }
        }
    }

    #[test]
    fn languages_differ_where_they_should() {
        assert_ne!(
            Msg::StartButton.text(Lang::En),
            Msg::StartButton.text(Lang::Tr)
        );
    }

    #[test]
    fn locale_lists_pick_the_first_supported_language() {
        assert_eq!(
            Lang::from_locales(["tr_TR.UTF-8", "tr_TR", "tr", "C"]),
            Lang::Tr
        );
        assert_eq!(Lang::from_locales(["en_US.UTF-8", "en"]), Lang::En);
        assert_eq!(Lang::from_locales(["de_DE.UTF-8", "tr_TR"]), Lang::Tr);
        assert_eq!(Lang::from_locales(["C"]), Lang::En);
        assert_eq!(Lang::from_locales([]), Lang::En);
        assert_eq!(Lang::from_locales(["trinidad"]), Lang::En);
    }

    #[test]
    fn explicit_settings_override_the_system() {
        assert_eq!(Lang::resolve("tr"), Lang::Tr);
        assert_eq!(Lang::resolve("en"), Lang::En);
    }
}
