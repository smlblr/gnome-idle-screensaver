# gnome-idle-screensaver

GNOME 46 Wayland için ekran koruyucu. Bilgisayar ayarlanan süre boşta kalınca tam ekran bir
animasyon gösterir; bir tuş, tıklama, kaydırma ya da belirgin fare hareketiyle kapanır. Ekranı
**kilitlemez**, GNOME'un kendi karartma/kilit ayarlarına dokunmaz.

İlk animasyon glxgears'ın üç çarkı (Mesa demos, Brian Paul, MIT). Kenar yumuşatma (MSAA),
her ekran oranına tam oturan izdüşüm ve yavaşça dönen kamera eklendi. Animasyonlar
değiştirilebilir modüllerdir; yenileri eklendikçe ayarlar penceresinde listelenir.

Neden gerekli: Wayland'de XScreenSaver boşta kalmayı göremez, GNOME 46'nın da ekran koruyucu
altyapısı yok. Boşta kalma Mutter'ın `org.gnome.Mutter.IdleMonitor` D-Bus arayüzünden izlenir.

## Kurulum

Gerekenler: Rust (1.92+), GTK 4.14 ve libadwaita 1.5 geliştirme paketleri.

```bash
sudo apt install libgtk-4-dev libadwaita-1-dev
```

Derleme ve kurulum (derlemeyi kısarak; `-j` değerini makinenize göre seçin):

```bash
nice -n 10 cargo build --release -j 6
```

```bash
./install.sh
```

`install.sh` sistem dizinlerine hiçbir şey yazmaz:

| Dosya | Yer |
|---|---|
| İkili | `~/.local/bin/gnome-idle-screensaver` |
| Uygulama girdisi | `~/.local/share/applications/io.github.smlblr.GnomeIdleScreensaver.desktop` |
| Otomatik başlatma | `~/.config/autostart/io.github.smlblr.GnomeIdleScreensaver.Daemon.desktop` |
| Simge | `~/.local/share/icons/hicolor/scalable/apps/io.github.smlblr.GnomeIdleScreensaver.svg` |
| Ayarlar (ilk değişiklikte oluşur) | `~/.config/gnome-idle-screensaver/config.toml` |

Betik izleyiciyi hemen başlatır (`--no-start` ile başlatmaz); sonraki oturumlarda izleyici
kendiliğinden başlar. GNOME Shell yeni simgeyi uygulama ızgarasında çoğu zaman ancak oturum
kapatılıp açılınca gösterir.

## Kaldırma

```bash
./uninstall.sh
```

İzleyiciyi durdurur, yukarıdaki dosyaları siler; ayar dosyası kalır. Onu da silmek için
`./uninstall.sh --purge`.

## Ayarlar

Uygulama ızgarasından **Ekran Koruyucu**'yu açın (ya da `gnome-idle-screensaver settings`).
Her değişiklik anında kaydedilir; izleyici dosyayı izler, yeniden başlatmak gerekmez.

| Ayar | `config.toml` anahtarı | Varsayılan |
|---|---|---|
| Ekran koruyucu açık/kapalı | `enabled` | `true` |
| Boşta kalma süresi (dakika, 1–720) | `idle_minutes` | `5` |
| Video oynarken başlama | `respect_inhibitors` | `true` |
| Animasyon (`gears`, `starfield`, `plasma`, `tunnel`, `aurora`) | `animation` | `"gears"` |

**Önizle** düğmesi seçili animasyonu hemen gösterir. İlk 0,8 saniyedeki girdi yok sayılır; böylece
düğmeye tıklayan el önizlemeyi hemen kapatmaz.

**Durum** bölümü izleyicinin çalışıp çalışmadığını gösterir (çalışmıyorsa **Başlat** düğmesi çıkar).
Ayrıca şu an boşta kalmayı engelleyenleri listeler. Engeli bir uygulama GNOME oturum
yöneticisinden istemişse adı görünür. Tarayıcılar ve Electron uygulamaları ise çoğu zaman
Wayland'in `idle-inhibit` protokolünü kullanır. Mutter bu istekleri tek bir anonim engelleyicide
toplar (`app_id = mutter`, `reason = idle-inhibit`) ve hangi pencerenin istediğini D-Bus'ta
göstermez. Bu durumda pencere yalnızca "görünür bir uygulama penceresi" der. Medya çalan bir
MPRIS oynatıcısı varsa onu "olası kaynak" olarak ekler; bu bir tahmindir, kanıt değildir.

### Animasyon seçenekleri (henüz arayüzde yok)

Her animasyonun seçenekleri kendi tablosundadır. Çarklar için varsayılanlar:

```toml
[animations.gears]
rotation_speed = 70.0   # büyük çarkın hızı, derece/sn (glxgears: 70)
camera_speed = 5.0      # kameranın dikey eksen etrafındaki dönüşü, derece/sn
colors = [[0.8, 0.1, 0.0], [0.0, 0.8, 0.2], [0.2, 0.2, 1.0]]
msaa_samples = 4        # sürücünün izin verdiği en yüksek değere kırpılır
```

Hatalı bir tablo animasyonu durdurmaz: günlüğe yazılır, varsayılanlar kullanılır.

### Animasyonlar

| Kimlik | Ad | Ne gösterir | Seçenek |
|---|---|---|---|
| `gears` | Çarklar | glxgears'ın üç çarkı, MSAA, yavaş dönen kamera | yukarıdaki tablo |
| `starfield` | Yıldızlar | Yıldızlı uzayda ileri uçuş, altı katman | `speed` |
| `plasma` | Plazma | Sinüs dalgalarından klasik renk akışı | `speed` |
| `tunnel` | Tünel | Dama desenli, hafifçe dönen sonsuz tünel | `speed` |
| `aurora` | Kuzey ışıkları | Yıldızlı gökte dalgalanan perdeler | `speed` |

`starfield`, `plasma`, `tunnel` ve `aurora` tek bir tam ekran parça gölgelendiricisidir
(`src/animations/fullscreen.rs`); MSAA kullanmazlar, çünkü kenar çizen bir geometri yoktur.
`speed` için `1.0` tasarım hızıdır:

```toml
[animations.plasma]
speed = 0.5
```

Ayarlar penceresini açmadan bir animasyona bakmak için:
`gnome-idle-screensaver preview --animation aurora --seconds 15`

## Komut satırı

```
gnome-idle-screensaver [settings|daemon|preview] [-v] [--seconds N]
```

- `preview --seconds 10`: seçili animasyonu 10 sn gösterir (girdiyle daha önce kapanır).
- `preview --animation KİMLİK`: ayardakini değil, verilen animasyonu gösterir; ayar dosyasına yazmaz.
- `-v`: kare hızı, odak ve kapanma gecikmesi günlüğe yazılır.
- `GNOME_IDLE_SCREENSAVER_CONFIG=/yol/config.toml`: başka bir ayar dosyası (test için).
- `cargo test --release -- --ignored`: her animasyonu ekran dışında çizer, GL hatası ve kare süresi
  denetler; `GIS_RENDER_DUMP=/dizin` ile kareleri PNG olarak kaydeder. Grafik oturum gerekir.

Çalışan izleyiciye elle gösterim yaptırmak için (parametre: kaç sn sonra kapansın, `0` = girdiye kadar):

```bash
gdbus call --session --dest io.github.smlblr.GnomeIdleScreensaver.Daemon --object-path /io/github/smlblr/GnomeIdleScreensaver/Daemon --method org.freedesktop.Application.ActivateAction show '[<uint32 10>]' '{}'
```

İzleyicinin günlüğü: `journalctl --user -b | grep gnome-idle-screensaver`.

## Mimari

Tek ikili, üç alt komut; her biri ayrı süreç:

```
                 config.toml  ◄── yazar ── settings (libadwaita penceresi)
                     │                         │ Önizle
              FileMonitor                      ▼
                     ▼                      Stage ──► GlScene (animasyon modülü)
 Mutter IdleMonitor ─► daemon ──────────────► Stage
 SessionManager ─────►   (boşta süre dolunca)
```

| Dosya | Görev |
|---|---|
| `src/daemon.rs` | Oturum boyunca çalışır. Mutter'a `AddIdleWatch` kurar, `WatchFired` gelince sahneyi açar. Engelleyicileri ve ayar dosyasını izler |
| `src/stage.rs` | Her monitöre bir tam ekran pencere. GLArea, kare saati, MSAA çerçeve arabelleği, kapatma kararı |
| `src/animations/` | Animasyon arayüzü (`GlScene`), kayıt tablosu (`ALL`), matris ve gölgelendirici yardımcıları |
| `src/animations/gears.rs` | glxgears çarkları |
| `src/settings.rs` | Ayarlar penceresi |
| `src/preview.rs` | `preview` alt komutu |
| `src/config.rs` | `config.toml` okuma/yazma (atomik), dosya izleme |
| `src/dbus.rs` | Mutter IdleMonitor ve GNOME SessionManager sarmalayıcıları |

### İzleyici

- **Boşta algılama anket yapmaz.** Mutter'a tek bir kalıcı `AddIdleWatch(idle_ms)` kurulur.
  Mutter onu her boşta kalma döneminde bir kez tetikler.
- **Engelleyiciler.** Mutter boşta-kalma engeli (oturum bayrağı 8) varken `AddIdleWatch`
  izlemelerini *hiç tetiklemez*. Bu makinede ölçüldü: kullanıcı 589 sn boştayken 3 sn'lik izleme
  12 sn içinde gelmedi, `GetIdletime` ise saymayı sürdürdü. Mutter'ın kaynak koduna göre engel
  kalkınca boşta sayacı da sıfırlanır; bu kısım henüz ölçülmedi.
  Sonuç olarak:
  - **"Video oynarken başlama" açıkken** (varsayılan) Mutter'ın davranışı zaten istenen davranış.
    İzleyici tetiklenme anında ayrıca `IsInhibited(8)` ile bir kez daha denetler. Askıya alma
    engeline (bayrak 4) bakılmaz.
  - **Kapalıyken** engel sürdüğü sürece izleme gelmez. İzleyici engel değişikliklerini
    (`InhibitorAdded/Removed`, `InhibitedActions`) dinler. Engel varsa `GetIdletime` ile kalan
    süreyi hesaplayıp tek atımlık bir zamanlayıcı kurar; süre dolunca bir kez daha denetler.
    Bu, eşik başına bir D-Bus çağrısı demektir; anket değildir. Sınır: engel kalktığında Mutter
    sayacı sıfırladığı için o andan itibaren süre yeniden başlar.
- **Ayar değişikliği**: `gio::FileMonitor` → 150 ms birleştirme → yeniden yükleme → izleme
  yeniden kurulur. Bozuk dosya, önceki ayarları değiştirmez.
- **GTK tembel başlatılır**: izleyici ilk gösterime kadar yalnızca bir D-Bus istemcisidir.
  Wayland bağlantısı, GTK ve GL sürücüsü ilk gösterimde yüklenir.
- Tek kopya: izleyici `io.github.smlblr.GnomeIdleScreensaver.Daemon` adını veri yolunda tutar.
  İkinci bir `daemon` hiçbir şey yapmadan çıkar.

### Tam ekran sahne ve odak

Sahne penceresi uzun ömürlü izleyici sürecinin içinde, `fullscreen_on_monitor` ile açılır.
Wayland'de istemci kendini öne getiremez; karar Mutter'ındır. Mutter'ın kaynak koduna göre
beklenen davranış şöyle; **bu makinede henüz ölçülmedi** (bkz. Ölçümler):
- Zaman damgası bilgisi olmayan yeni bir pencere odak alır ve öne yerleşir.
- Odaklı pencere bir terminalse yalnızca öne yerleşir, odak terminalde kalır.
- `present()` ayrıca bir xdg-activation isteği gönderir. Son girdi seri numarası taşımayan bu
  istek reddedilir ve "… hazır" bildirimine dönüşebilir.

İzleyicinin penceresi bu yüzden `present()` ile değil `set_visible(true)` ile açılır.

Kapatma:
- Tuş, tıklama, kaydırma: hemen.
- Fare: bir hareket dizisinin başladığı noktadan 24 mantıksal pikseli aşınca. Hareket 0,8 sn
  durursa dizi yeniden başlar; yavaş sensör kayması birikmez.
- Klavye odağı başka pencerede kaldıysa tuşlar bize gelmez. Bunun için sahne açıkken Mutter'a
  bir `AddUserActiveWatch` kurulur. Mutter girdi bildirir ama aynı girdi 150 ms içinde
  pencerelerimize ulaşmamışsa sahne kapanır. Küçük fare titremesi pencereye ulaştığı için bu yoldan
  sahneyi kapatmaz.

### Animasyon modülü arayüzü

```rust
pub trait GlScene {
    fn realize(&mut self, gl: &glow::Context, glsl: GlslDialect) -> Result<(), String>;
    fn render(&mut self, gl: &glow::Context, frame: &Frame);
    fn unrealize(&mut self, gl: &glow::Context);
    fn msaa_samples(&self) -> i32 { 4 }
}
```

Sahne (`stage.rs`) modül yerine şunları yapar: GL bağlamını hazırlar, çok örnekli çerçeve
arabelleğini bağlar ve görüntü alanını ayarlar. Kare saatinden (dikey senkron) zamanı verir:
`Frame { elapsed, delta, width, height }`. Sonra örnekleri GtkGLArea'nın arabelleğine çözer.
Modül yalnızca çizer.

GTK4'ün GLArea'sı ya masaüstü **core profile** ya da **GLES 3** bağlamı verir;
`glBegin/glEnd`, görüntü listeleri ve sabit işlevli ışık yoktur. Gölgelendiriciler iki lehçenin
ortak alt kümesinde yazılır. `GlslDialect::header()` başlarına `#version 150` ya da
`#version 300 es` ekler. GL işlev işaretçileri GTK'nın zaten yüklü olan libepoxy'sinden
çözülür (glow ile).

### Yeni bir animasyon eklemek

1. `src/animations/benim.rs` oluşturun: `GlScene`'i uygulayan bir yapı ve bir `INFO`:

   ```rust
   pub const INFO: AnimationInfo = AnimationInfo { id: "benim", title: "Benim animasyonum", create };

   fn create(options: Option<&toml::Value>) -> Box<dyn GlScene> {
       Box::new(Benim::new(super::parse_options(INFO.id, options)))
   }
   ```

2. `src/animations/mod.rs`: `pub mod benim;` ekleyin, `ALL` dizisine `benim::INFO` yazın.
3. Seçenekleri varsa `#[derive(Deserialize, Default)]` bir yapı yazın; `[animations.benim]`
   tablosundan okunur. `config.rs`'e dokunmak gerekmez.

Yalnızca bir parça gölgelendiricisi yazacaksanız `GlScene`'i elle uygulamayın:
`fullscreen::FullscreenScene::new(FRAGMENT, hız)` verilen gölgelendiriciyi `u_time` ve
`u_resolution` uniform'larıyla çalıştırır; örnek `plasma.rs` (20 satır).

Ayarlar penceresindeki seçim listesi `ALL`'dan dolar; kayıt dışında başka bir yeri değiştirmek
gerekmez. Örnek olarak `gears.rs`'e bakın.

### Kararlar

- **Neden TOML, GSettings değil.** Seçenekler zamanla modül modül eklenecek. GSettings'te her yeni
  anahtar için şema değişikliği, `glib-compile-schemas` ve derlenmiş şemanın
  `~/.local/share/glib-2.0/schemas` altına kurulması gerekirdi. Modül başına seçenek tabloları ise
  ya ayrı ayrı yeniden konumlandırılabilir şemalar ya da `a{sv}` sözlükleri demek. TOML dosyasında
  her modül kendi `[animations.<id>]` tablosunu kendisi ayrıştırır. Dosya elle düzenlenebilir,
  kaldırma tek bir dizini silmektir. GSettings'in bedava verdiği değişiklik bildirimini burada
  `gio::FileMonitor` sağlar; ayarlar penceresi atomik yazar (geçici dosya + `rename`).
- **Neden zbus değil, gio D-Bus.** GTK zaten GLib ana döngüsünde. gio'nun D-Bus'ı aynı döngüde,
  aynı iş parçacığında çalışır; ayrı bir async çalışma zamanı gerekmez.
- **Neden `epoxy` paketi değil.** Paket bakımsız: geri çekilmiş bir `xml-rs` sürümüne bağlı olduğu
  için derlenmiyor. Aynı işi, GTK'nın yüklediği libepoxy'den `epoxy_gl*` işaretçilerini libloading
  ile okumak yapıyor.
- **Intel GPU, NVIDIA değil.** PRIME offload değişkenleri kullanılmaz; GTK'nın bağlamı varsayılan
  EGL aygıtında (Intel UHD 630) açılır. Günlükteki `GL:` satırı sürücüyü gösterir.

## Ölçümler (MSI GP63, Ubuntu 24.04, GNOME 46 Wayland, Intel UHD 630)

| Ölçüm | Sonuç |
|---|---|
| İzleyici boşta, ilk gösterimden önce | 60 sn'de **%0,00 CPU** (hiç CPU tiki yok), **RSS 22,5 MB**, 4 iş parçacığı |
| İkinci `daemon` | "already running" der, 0 ile çıkar |
| Ayar değişikliği → izleyici | Yeniden başlatmadan yansıyor: yerinde düzenleme, `rename` ile değiştirme, `enabled` aç/kapa. Bozuk TOML'da önceki ayarlar korunuyor. Hepsi 1,5 sn'lik gözlem aralığında |
| Mutter izlemesi, engel varken | 589 sn boştayken 3 sn'lik `AddIdleWatch` 12 sn içinde tetiklenmedi |
| GL bağlamı (GTK'nın seçimi) | **OpenGL ES 3.2**, Mesa 25.2.8, Intel UHD 630 (CFL GT2). NVIDIA kullanılmıyor |
| Çizim, ekran dışı (`cargo test -- --ignored`) | 1920×1080, 4x MSAA: **1,46 ms/kare**, her karede GPU boşaltılarak. GL hatası yok. Kenarlarda ara tonlar var (MSAA çözümlemesi çalışıyor) |
| Ekran dışı kare süresi, 1920×1080 | gears 1,6 · plazma 1,5 · tünel 2,0 · kuzey ışıkları 4,4 · yıldızlar 4,6 ms (60 Hz bütçesi 16,7 ms) |
| Derleme | `nice -n 10 cargo build --release -j 2`, yaklaşık 3 dk. Makinede başka bir süreç 5,5 çekirdek kullanırken yük en fazla 9,05, en yüksek 86 °C |

**Henüz ölçülmedi** (ekranı kaplayan testler ertelendi):
- Ekranda kare hızı ve dikey senkron kilidi, animasyon sırasında CPU.
- İlk gösterimden sonra izleyicinin RSS'i (GL sürücüsü yüklendikten sonra).
- Boşta süre dolunca pencerenin gerçekten öne gelip odak alması; dokununca kapanma gecikmesi.
- Engel varken başlamaması, anahtar kapalıyken başlaması.
- Oturum kapatıp açınca otomatik başlama.

### Ekran testleri için araçlar (`tools/`)

- `tools/test-config.toml`: 1 dk eşikli test ayarı. `GNOME_IDLE_SCREENSAVER_CONFIG` ile verilir;
  asıl ayar dosyasına dokunmaz.
- `tools/inject.py move:3,2 sleep:1 move:120,0 key:0xffe1`: Mutter RemoteDesktop üzerinden yapay
  girdi. Günlükteki kapanma satırıyla karşılaştırmak için enjeksiyon zamanını CLOCK_MONOTONIC
  olarak yazar.
- `tools/shot.py çıktı.png`: portal üzerinden etkileşimsiz ekran görüntüsü; dosyayı ~/Pictures'tan
  alıp taşır.
- `tools/cpu.sh PID SANİYE`: sürecin o aralıktaki CPU %'si (100 = bir çekirdek) ve RSS'i.

## Bilinen sınırlar

- Çoklu monitör: her monitöre bir pencere açılır, ama bu makinede tek ekran olduğu için denenmedi.
- Kilitlemez; GNOME kilidi açıksa kendi süresinde ayrıca devreye girer.
- Mutter'ın anonim `idle-inhibit` engelleyicisinin sahibi D-Bus'tan öğrenilemez (yukarıya bakın).

## Lisans

`src/animations/gears.rs`, Mesa demos'taki glxgears.c'nin uyarlamasıdır:
Copyright (C) 1999-2001 Brian Paul, MIT (`LICENSES/MIT-mesa-demos.txt`). Projenin geri kalanının
lisansı henüz belirlenmedi.
