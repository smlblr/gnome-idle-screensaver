#[macro_use]
mod log;

mod animations;
mod config;
mod daemon;
mod dbus;
mod i18n;
mod preview;
mod settings;
mod stage;

use gtk::glib;

const USAGE_EN: &str = "\
Usage: gnome-idle-screensaver [COMMAND] [OPTIONS]

Commands:
  settings          Open the settings window (default)
  daemon            Run the idle watcher (started with the session)
  preview           Show the selected animation full screen

Options:
  -v, --verbose     Verbose log: frame rate, focus, close latency
  --seconds N       preview: close by itself after N seconds
  --animation ID    preview: show this one instead of the configured one
                    (gears, starfield, plasma, tunnel, aurora, blank)
  -h, --help        Show this help

Environment:
  GNOME_IDLE_SCREENSAVER_CONFIG   Path of the config file (for testing)
";

const USAGE_TR: &str = "\
Kullanım: gnome-idle-screensaver [KOMUT] [SEÇENEKLER]

Komutlar:
  settings          Ayarlar penceresini açar (varsayılan)
  daemon            Boşta kalma izleyicisini çalıştırır (oturumla başlar)
  preview           Seçili animasyonu tam ekran gösterir

Seçenekler:
  -v, --verbose     Ayrıntılı günlük: kare hızı, odak, kapanma gecikmesi
  --seconds N       preview: N saniye sonra kendiliğinden kapan
  --animation ID    preview: ayardakini değil bunu göster
                    (gears, starfield, plasma, tunnel, aurora, blank)
  -h, --help        Bu yardımı gösterir

Ortam değişkeni:
  GNOME_IDLE_SCREENSAVER_CONFIG   Ayar dosyasının yolu (test için)
";

fn main() -> glib::ExitCode {
    let lang = i18n::Lang::resolve(&config::load().map(|c| c.language).unwrap_or_default());
    let usage = lang.pick(USAGE_EN, USAGE_TR);
    let mut command: Option<String> = None;
    let mut seconds: Option<u32> = None;
    let mut animation: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-v" | "--verbose" => log::set_verbose(true),
            "--seconds" => match args.next().and_then(|s| s.parse().ok()) {
                Some(n) => seconds = Some(n),
                None => {
                    eprintln!(
                        "{}\n\n{usage}",
                        lang.pick("--seconds expects a number", "--seconds bir sayı bekliyor")
                    );
                    return glib::ExitCode::FAILURE;
                }
            },
            "--animation" => match args
                .next()
                .filter(|id| animations::ALL.iter().any(|a| a.id == id))
            {
                Some(id) => animation = Some(id),
                None => {
                    eprintln!(
                        "{}\n\n{usage}",
                        lang.pick(
                            "--animation expects a known id",
                            "--animation bilinen bir kimlik bekliyor"
                        )
                    );
                    return glib::ExitCode::FAILURE;
                }
            },
            "-h" | "--help" => {
                print!("{usage}");
                return glib::ExitCode::SUCCESS;
            }
            "settings" | "daemon" | "preview" if command.is_none() => command = Some(arg),
            other => {
                eprintln!(
                    "{}{other}\n\n{usage}",
                    lang.pick("Unknown argument: ", "Bilinmeyen argüman: ")
                );
                return glib::ExitCode::FAILURE;
            }
        }
    }

    match command.as_deref().unwrap_or("settings") {
        "daemon" => daemon::run(),
        "preview" => preview::run(seconds, animation),
        _ => settings::run(),
    }
}
