# gnome-idle-screensaver — çalışma kuralları

GNOME 46 Wayland için ekran koruyucu. Rust + gtk4-rs + libadwaita, tek ikili (`daemon`,
`settings`, `preview`). Mimari ve kurulum README.md'de.

- Yanıtlar ve dokümanlar Türkçe; kod, tanımlayıcılar ve günlük (log) iletileri İngilizce.
  Arayüz metinleri Türkçe.
- Hiçbir şey sistem dizinine kurulmaz; yalnızca `~/.local` ve `~/.config` (install.sh).
- Derleme 30 sn'yi aşarsa kıs: `nice -n 10 cargo build --release -j <N>`, N'yi ölçerek seç.
- Doğrulama gerçek oturumda ölçümdür (CPU, RSS, kare hızı, kapanma gecikmesi); tip denetimi yetmez.

## Yorum politikası

1. **Ne değil, neden.** Kodun zaten söylediğini yorum tekrar etmez.
2. **En fazla üç satır.** Gerekçe sığmıyorsa bir belgeye aittir; yorum o belgenin adresidir.
3. **Hikâye varsa adresi yazılır, hikâye değil**: README'deki bölüm başlığı ya da `docs/`
   altındaki not. Tek satır.
4. **Adres çözülmeli.** "Belgelere bakın" yetmez; gerekçenin henüz yeri yoksa aynı commit'te
   yazılır, sonra yorum kısaltılır. Kısaltmak bilgiyi taşır, silmez.

Üçüncü taraf lisans başlıkları (ör. `src/animations/gears.rs` içindeki Mesa demos / Brian Paul
MIT başlığı) bu kuralın dışındadır ve olduğu gibi korunur.
