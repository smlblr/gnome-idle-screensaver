# Mimari

[English](architecture.md) · [Türkçe](architecture.tr.md)

Tek ikili, üç alt komut; her biri ayrı süreç:

```
                 config.toml  ◄── yazar ── settings (libadwaita penceresi)
                     │                         │ Önizle
              FileMonitor                      ▼
                     ▼                      Stage ──► GlScene (bir animasyon)
 Mutter IdleMonitor ─► daemon ──────────────► Stage
 SessionManager ─────►   (boşta süre dolunca)
```

| Dosya | Görev |
|---|---|
| `src/daemon.rs` | Oturum boyunca çalışır. Mutter'a `AddIdleWatch` kurar, `WatchFired` gelince sahneyi açar. Engelleyicileri ve ayar dosyasını izler |
| `src/stage.rs` | Her monitöre bir tam ekran pencere: GLArea, kare saati, MSAA çerçeve arabelleği, kapatma kuralları |
| `src/animations/` | `GlScene` arayüzü, kayıt tablosu (`ALL`), matris ve gölgelendirici yardımcıları, animasyon başına bir dosya |
| `src/settings.rs` | Ayarlar penceresi |
| `src/preview.rs` | `preview` alt komutu |
| `src/config.rs` | `config.toml` okuma/yazma (atomik), dosya izleme |
| `src/dbus.rs` | Mutter IdleMonitor ve GNOME SessionManager sarmalayıcıları |
| `src/i18n.rs` | Arayüz dilleri |

## İzleyici

- **Anket yok.** Mutter'a tek bir kalıcı `AddIdleWatch(idle_ms)` kurulur; Mutter onu her boşta
  kalma döneminde bir kez tetikler.
- **Engelleyiciler.** Boşta-kalma engeli (oturum bayrağı 8) varken Mutter boşta izlemelerini
  *hiç tetiklemez*. Ölçüldü: kullanıcı 589 sn boştayken 3 sn'lik izleme 12 sn içinde gelmedi,
  `GetIdletime` ise saymayı sürdürdü. Mutter'ın kaynak koduna göre engel kalkınca boşta sayacı da
  sıfırlanır. Sonuç:
  - **"Video oynarken başlama" açıkken** (varsayılan) Mutter'ın davranışı istenen davranıştır.
    İzleyici tetiklenme anında ayrıca `IsInhibited(8)` ile bir kez daha denetler. Askıya alma
    engeline (bayrak 4) bilerek bakılmaz.
  - **Kapalıyken** engel sürdükçe izleme gelmez. İzleyici engel değişikliklerini
    (`InhibitorAdded/Removed`, `InhibitedActions`) dinler; engel varsa `GetIdletime` ile kalan
    süreyi hesaplayıp tek atımlık bir zamanlayıcı kurar, süre dolunca yeniden denetler. Eşik başına
    bir D-Bus çağrısıdır, anket değil.
- **Ayar değişikliği:** `gio::FileMonitor` → 150 ms birleştirme → yeniden yükleme → izleme yeniden
  kurulur. Bozuk dosya önceki ayarları korur.
- **GTK tembel başlatılır:** ilk gösterime kadar izleyici küçük bir D-Bus istemcisidir (yaklaşık
  23 MB, %0 CPU). Wayland, GTK ve GL sürücüsü ilk gösterimde yüklenir.
- **Tek kopya:** izleyici oturum veri yolunda `io.github.smlblr.GnomeIdleScreensaver.Daemon` adını
  tutar; ikinci bir `daemon` hiçbir şey yapmadan çıkar.
- Betikler ve testler için çalışan izleyici, sahneyi gösteren bir eylem kabul eder (parametre:
  kendiliğinden kapanmadan önceki saniye, `0` = girdiye kadar):

  ```bash
  gdbus call --session --dest io.github.smlblr.GnomeIdleScreensaver.Daemon --object-path /io/github/smlblr/GnomeIdleScreensaver/Daemon --method org.freedesktop.Application.ActivateAction show '[<uint32 10>]' '{}'
  ```

## Sahne, odak ve kapanma

Sahne penceresi uzun ömürlü izleyicinin içinde `fullscreen_on_monitor` ile açılır. Wayland'de
istemci kendini öne getiremez; karar pencere yöneticisinindir. Mutter'ın kaynak koduna göre
kurallar:

- Zaman damgası bilgisi olmayan yeni pencere odak alır ve öne yerleşir.
- Odaklı pencere bir terminalse yalnızca öne yerleşir, odak terminalde kalır.
- `present()` ayrıca bir xdg-activation isteği gönderir. Yakın tarihli girdi seri numarası yoksa
  Mutter bunu reddeder ve "… hazır" bildirimine dönüşebilir.

Bu yüzden boşta tetiklenen sahne `present()` ile değil `set_visible(true)` ile açılır ve izleyici
devraldığı `XDG_ACTIVATION_TOKEN`'ı atar. Test makinesinde pencere fiilen odak aldı (açıldıktan bir
saniye sonra `active=true`).

**Kapatma:**

- Tuş, tıklama, kaydırma: hemen.
- Fare: işaretçi, bir hareket dizisinin başladığı noktadan 24 mantıksal piksel giderse. Dizi,
  işaretçinin son *durduğu* konumda başlar, yani tek büyük sıçrama da sayılır; 0,8 sn
  hareketsizlikten sonra biter, böylece yavaş sensör kayması birikmez. GTK'nın pencereye girişte
  bildirdiği konum kullanılmaz: yeni açılan pencerenin altında gerçek konumdan çok uzak olabilir.
- Klavye odağı başka pencerede kalabilir, o zaman tuşlar sahneye hiç ulaşmaz. Bu yüzden sahne
  açıkken izleyici Mutter'a bir `AddUserActiveWatch` tutar. Mutter'ın bildirdiği etkinlik 150 ms
  içinde pencerelerimize ulaşmazsa sahne kapanır. Açıldıktan sonraki ilk 300 ms'deki etkinlik ve
  önizlemede ilk 0,8 sn'deki girdi (onu isteyen tıklama) yok sayılır.

## Animasyon arayüzü

```rust
pub trait GlScene {
    fn realize(&mut self, gl: &glow::Context, glsl: GlslDialect) -> Result<(), String>;
    fn render(&mut self, gl: &glow::Context, frame: &Frame);
    fn unrealize(&mut self, gl: &glow::Context);
    fn msaa_samples(&self) -> i32 { 4 }
    fn animated(&self) -> bool { true }
}
```

Sahne GL bağlamını hazırlar, çok örnekli çerçeve arabelleğini bağlar, görüntü alanını ayarlar ve
kare saatinden (dikey senkron) zamanı verir: `Frame { elapsed, delta, width, height }`. Sonra
örnekleri GtkGLArea'nın kendi arabelleğine çözer. Animasyon yalnızca çizer. `animated() == false`
dönen sahne üç kez çizilir, sonra kendi hâline bırakılır; siyah "kapalı ekran"ı bedava yapan budur.

GTK4'ün GLArea'sı ya masaüstü **core profile** ya da **GLES 3** bağlamı verir: `glBegin/glEnd`,
görüntü listeleri, sabit işlevli ışık yoktur. Gölgelendiriciler iki lehçenin ortak alt kümesinde
yazılır; `GlslDialect::header()` başlarına `#version 150` ya da `#version 300 es` ekler. GL işlev
işaretçileri, GTK'nın zaten yüklediği libepoxy'den `glow` ile çözülür.

### Animasyon eklemek

1. `src/animations/benim.rs` oluşturun: `GlScene`'i uygulayan bir yapı ve bir `INFO`:

   ```rust
   pub const INFO: AnimationInfo = AnimationInfo {
       id: "benim",
       title_en: "My animation",
       title_tr: "Benim animasyonum",
       create,
   };

   fn create(options: Option<&toml::Value>) -> Box<dyn GlScene> {
       Box::new(Benim::new(super::parse_options(INFO.id, options)))
   }
   ```

2. `src/animations/mod.rs`'e `pub mod benim;` ekleyin, `ALL` dizisine `benim::INFO` yazın.
3. Seçenekleri varsa `#[derive(Deserialize, Default)]` bir yapı yazın; `[animations.benim]`
   tablosundan okunur. `config.rs`'e dokunmak gerekmez.

Ayarlar penceresindeki liste `ALL`'dan dolar; başka bir yeri değiştirmek gerekmez.

Yalnızca bir parça gölgelendiricisi istiyorsanız `GlScene`'i elle yazmayın:
`fullscreen::FullscreenScene::new(FRAGMENT, hız)` gölgelendiriciyi `u_time` ve `u_resolution`
uniform'larıyla çalıştırır; örnek `plasma.rs`, yaklaşık 20 satır. `cargo test` sonucu göremez;
`tools/gui-tests.sh` çalıştırın: her animasyonu ekran dışında çizer, GL hatası ve kapsamı denetler,
kare sürelerini ölçer; `GIS_RENDER_DUMP=/dizin` kareleri PNG olarak kaydeder.

## Dil değiştirme

Arayüzün iki dili var, İngilizce ve Türkçe; `language` ayarı seçer (`auto` masaüstünün dilini
izler: `LANGUAGE`, `LC_ALL`, `LC_MESSAGES`, `LANG`). Günlük satırları her zaman İngilizcedir.

- Metinler `src/i18n.rs` içindeki tek bir `messages!` tablosundadır. `Msg::text` kapsamlı bir
  `match`'tir; dillerden birinde eksik metin derlenmez.
- Ayarlar penceresindeki her kullanıcıya görünen metin `Ui::retranslate()` içinde atanır; dil
  değişince açık pencere yeniden başlatmadan güncellenir.
- **`retranslate()`'i bir notify işleyicisinin içinden çağırmayın.** Bir açılır liste modelini
  yeniden kurmak seçimi sıfırlar ve `notify::selected` üretir. GObject, aynı özellik için bir
  işleyici çalışırken üretilen bildirimleri tutar ve sonradan, "koruma" bayrağı çoktan bırakıldığında
  iletir; işleyici yeniden çalıştı, modeli yeniden kurdu ve sonsuza dek döndü. Dil işleyicisi
  `retranslate()`'i boşta çalışan bir geri çağırıma erteler, dil satırının kendi modeli hiç yeniden
  kurulmaz (etiketleri sabit ve iki dillidir), ayar değişmemişse işleyici hemen döner.
  `settings::tests` bunu kapsar; `tools/gui-tests.sh` ile çalıştırın.
- Animasyon adları `AnimationInfo` üzerinde dil başınadır (`title_en`, `title_tr`); `.desktop`
  dosyaları `[tr]` anahtarları taşır.

### Dil eklemek

1. `Lang`'e bir değer ve `Lang::from_locales`'e yerel ayar önekini ekleyin.
2. `pick()` ve `messages!` tablosu, her mesajda yeni metin olana dek derlenmez.
3. Dili `i18n::SETTINGS` ve `LANGUAGE_NAMES`'e, `AnimationInfo`'ya bir `title_xx`'i ve iki
   `.desktop.in` dosyasına `[xx]` anahtarlarını ekleyin.

Neden gettext değil: iki dil ve yaklaşık otuz kısa metin `.po` dosyalarını, derleme zamanı bir
adımı ve libintl bağını haklı çıkarmaz; gettext ayrıca çalışan pencerede yeniden başlatmadan dil
değiştiremez.

## Pencerenin kapatılması

Sahne kapanırken pencereleri, kapatma kararını veren işleyicinin içinde yok edilmez. 2026-09-28'de,
gerçek bir boşta gösterimden hemen sonra, izleyici kapandıktan iki saniye sonra `libgtk-4`
içinde çöktü (çekirdek günlüğünde segfault): fare olayı hâlâ işlenirken penceresi o işleyicide yok
edilmiş, ardından Wayland gidiş-dönüşü yapılmıştı. `Inner::close` artık yalnızca kararı kaydeder,
`destroy()` ve gidiş-dönüşü ana döngünün bir sonraki turuna erteler (`glib::idle_add_local_once`).
Yedi ardışık açma/kapama ve sonraki gerçek kullanım çökmeden çalıştı. Aynı anda bir RDP bağlantısı
kuruluyordu, tetikleyici o olabilir; ertelenmiş kapatma her iki durumda da doğrudur.

## Tasarım kararları

- **TOML, GSettings değil.** Seçenekler modül modül eklenir. GSettings'te her yeni anahtar için
  şema değişikliği, `glib-compile-schemas` ve derlenmiş şemanın `~/.local/share/glib-2.0/schemas`
  altına kurulması gerekirdi. TOML dosyasında her modül kendi `[animations.<id>]` tablosunu
  kendisi ayrıştırır. Dosya elle düzenlenebilir, kaldırma tek bir dizini silmektir. GSettings'in
  bedava verdiği değişiklik bildirimini `gio::FileMonitor` sağlar; ayarlar penceresi atomik yazar
  (geçici dosya + `rename`).
- **zbus değil, gio D-Bus.** GTK zaten GLib ana döngüsünde; gio'nun D-Bus'ı aynı döngüde, aynı
  iş parçacığında çalışır, ayrı bir async çalışma zamanı gerekmez.
- **`epoxy` paketi yok.** Geri çekilmiş bir `xml-rs` sürümüne bağlı olduğu için derlenmiyor. Aynı işi, GTK'nın yüklediği libepoxy'den `epoxy_gl*` işaretçilerini `libloading` ile
  okumak yapıyor.
- **Intel GPU, NVIDIA değil.** PRIME offload değişkenleri kullanılmaz; GTK'nın bağlamı varsayılan
  EGL aygıtında açılır.
- **Sahne izleyicinin içinde yaşar.** Her gösterim için ayrı süreç izleyiciyi küçük tutardı ama
  sahne penceresine yukarıdaki odak sorunlarını getirirdi. Bedeli, ilk gösterimden sonra izleyicinin
  belleğidir: bkz. [ölçümler](measurements.md).
