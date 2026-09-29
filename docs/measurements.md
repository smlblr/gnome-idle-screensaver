# Measurements / Ölçümler

Machine / Makine: MSI GP63 Leopard 8RE, Ubuntu 24.04, GNOME 46 Wayland, Intel UHD 630 (the
default GPU / varsayılan GPU), one 1920×1080 monitor at 60 Hz / tek monitör.

Measured in a real window with `preview --animation X -v`, 8 s samples. CPU % is per core
(100 = one core) on a 12-thread machine, so the highest value below is about 0.5 % of the whole
CPU. / Gerçek pencerede, 8 sn'lik örnekler. CPU yüzdesi çekirdek başına (100 = bir çekirdek).

| Animation / Animasyon | CPU | Frame rate, steady / Kare hızı | Offscreen frame time / Ekran dışı kare süresi |
|---|---|---|---|
| gears / çarklar | 4.9 % | 59.6–60.0 | 1.2–1.7 ms |
| starfield / yıldızlar | 2.9 % | 59.2–59.8 | 4.5 ms |
| plasma / plazma | 5.6 % | 59.6–59.7 | 1.5 ms |
| tunnel / tünel | 5.8 % | 59.6–60.0 | 1.9 ms |
| aurora / kuzey ışıkları | 4.3 % | 59.7–59.8 | 4.3 ms |
| blank / kapalı ekran | 0.0 % | not drawn / çizilmiyor | 0.5 ms |

The frame rate follows the frame clock (60 Hz vertical sync). The first 5 s window reads higher
because it also counts the opening frames; the numbers above are the ones after it. /
İlk 5 sn'lik pencere açılış karelerini de saydığı için yüksek görünür; yukarıdakiler ondan sonrakilerdir.

| Measurement / Ölçüm | Result / Sonuç |
|---|---|
| Watcher before the first show / İzleyici, ilk gösterimden önce | 0.00 % CPU over 60 s, RSS 22.9 MB |
| Watcher while showing / Gösterim sırasında | RSS 142 MB (PSS 49 MB: shared GTK and Mesa libraries are counted in RSS) |
| Watcher after the show closed / Kapandıktan sonra | RSS stays near 140 MB (the libraries are not released), CPU 0.00 % |
| Close latency, key / Kapanma gecikmesi, tuş | input→handler 0.6–1.5 ms, →Mutter ack 9–40 ms |
| Close latency, mouse / fare | 2–14 ms and 15–40 ms |
| Small jitter (3 px) / Küçük titreme | did not close, 5 of 5 |
| Deliberate motion (250 px) / Belirgin hareket | closed, 5 of 5 |
| Focus / Odak | the window took focus (`active=true` one second after opening) |
| Inhibitor present, switch on / Engelleyici var, anahtar açık | did not start while idle for 67 s (threshold 60 s) |
| Same, switch turned off / Anahtar kapatılınca | reloaded from the file and started at once |
| 7 consecutive show/close cycles / ardışık açma-kapama | no crash |
| Autostart at login / Oturumda otomatik başlama | observed 2026-09-28, opened by itself after 5 min |
| Screenshot / Ekran görüntüsü | real full screen confirmed: no top bar or border |
| Settings window | language switch English ↔ Türkçe covered by an automated test (`tools/gui-tests.sh`) |
| Build / Derleme | `nice -n 10 cargo build --release -j 2`, about 3 min; peak load 9.05, 86 °C |

**Not measured / Ölçülmedi:** the 1-minute idle wait itself with *no* inhibitor present (this
session had an inhibitor that could not be removed; a real idle show is in the 2026-09-28 log),
an extra inhibitor made with `gnome-session-inhibit`, several monitors, other GPUs.
/ Engelleyici *yokken* 1 dk'lık boşta beklemenin kendisi, `gnome-session-inhibit` ile ek engelleyici,
çoklu monitör, başka ekran kartları.
