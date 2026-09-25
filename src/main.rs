#[macro_use]
mod log;

mod animations;
mod config;
mod daemon;
mod dbus;
mod preview;
mod stage;

use gtk::glib;

const USAGE: &str = "\
Kullanım: gnome-idle-screensaver [KOMUT] [SEÇENEKLER]

Komutlar:
  settings          Ayarlar penceresini açar (varsayılan)
  daemon            Boşta kalma izleyicisini çalıştırır (oturumla başlar)
  preview           Seçili animasyonu tam ekran gösterir

Seçenekler:
  -v, --verbose     Ayrıntılı günlük: kare hızı, odak, kapanma gecikmesi
  --seconds N       preview: N saniye sonra kendiliğinden kapan
  -h, --help        Bu yardımı gösterir

Ortam değişkeni:
  GNOME_IDLE_SCREENSAVER_CONFIG   Ayar dosyasının yolu (test için)
";

fn main() -> glib::ExitCode {
    let mut command: Option<String> = None;
    let mut seconds: Option<u32> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-v" | "--verbose" => log::set_verbose(true),
            "--seconds" => match args.next().and_then(|s| s.parse().ok()) {
                Some(n) => seconds = Some(n),
                None => {
                    eprintln!("--seconds bir sayı bekliyor\n\n{USAGE}");
                    return glib::ExitCode::FAILURE;
                }
            },
            "-h" | "--help" => {
                print!("{USAGE}");
                return glib::ExitCode::SUCCESS;
            }
            "daemon" | "preview" if command.is_none() => command = Some(arg),
            other => {
                eprintln!("Bilinmeyen argüman: {other}\n\n{USAGE}");
                return glib::ExitCode::FAILURE;
            }
        }
    }

    match command.as_deref().unwrap_or("") {
        "daemon" => daemon::run(),
        "preview" => preview::run(seconds),
        _ => {
            eprintln!("{USAGE}");
            glib::ExitCode::FAILURE
        }
    }
}
