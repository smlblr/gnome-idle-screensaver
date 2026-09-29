# gnome-idle-screensaver — çalışma kuralları

GNOME 46 Wayland için ekran koruyucu. Rust + gtk4-rs + libadwaita, tek ikili (`daemon`,
`settings`, `preview`). Kurulum README.md'de, mimari docs/architecture.md'de.

- Sohbet yanıtları Türkçe. Kod, tanımlayıcılar ve günlük (log) iletileri İngilizce. Arayüz iki
  dilli (İngilizce, Türkçe; src/i18n.rs). README ve docs/architecture İngilizce ve Türkçe
  tutulur, biri değişince öteki de güncellenir.
- `install.sh` sistem dizinine yazmaz, yalnızca `~/.local` ve `~/.config`. `.deb` standart
  sistem yollarına kurulur (packaging/build-deb.sh); ikisini birlikte kurmayın.
- Derleme 30 sn'yi aşarsa kıs: `nice -n 10 cargo build --release -j <N>`, N'yi ölçerek seç.
- Doğrulama gerçek oturumda ölçümdür (CPU, RSS, kare hızı, kapanma gecikmesi); tip denetimi yetmez.
  Ekran gerektiren testler `tools/gui-tests.sh` ile çalışır; `cargo fmt` ve `cargo clippy` temiz olmalı.

## Yorum politikası

1. **Ne değil, neden.** Kodun zaten söylediğini yorum tekrar etmez.
2. **En fazla üç satır.** Gerekçe sığmıyorsa bir belgeye aittir; yorum o belgenin adresidir.
3. **Hikâye varsa adresi yazılır, hikâye değil**: `docs/architecture.md` içindeki bölüm başlığı
   (ör. "Window teardown"). Tek satır.
4. **Adres çözülmeli.** "Belgelere bakın" yetmez; gerekçenin henüz yeri yoksa aynı commit'te
   yazılır, sonra yorum kısaltılır. Kısaltmak bilgiyi taşır, silmez.

Üçüncü taraf lisans başlıkları (ör. `src/animations/gears.rs` içindeki Mesa demos / Brian Paul
MIT başlığı) bu kuralın dışındadır ve olduğu gibi korunur.
