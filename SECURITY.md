# Güvenlik Politikası

## Desteklenen sürümler

Hydra DL erken geliştirme aşamasındadır. Güvenlik düzeltmeleri yalnızca en son sürüm
daline (`main`) ve yayımlanmış en yeni sürüme uygulanır.

| Sürüm | Destek |
|---|---|
| `main` (en son sürüm) | ✅ |
| Eski sürümler | ❌ |

## Güvenlik açığı bildirme

**Lütfen güvenlik açıklarını herkese açık issue, PR veya tartışma olarak paylaşmayın.**

Bunun yerine GitHub'ın özel güvenlik bildirimi kanalını kullanın:

👉 <https://github.com/TitanLion72/hydra-dl/security/advisories/new>

Bildirime şunları ekleyin:

- Etkilenen sürüm veya commit
- Sorunun türü (ör. yol aşımı, komut enjeksiyonu, yetkisiz erişim, XSS)
- Yeniden üretme adımları veya kavram kanıtı (PoC)
- Olası etki ve sizin değerlendirdiğiniz önem derecesi
- Varsa önerdiğiniz düzeltme
- Bize nasıl atıf yapılmasını istediğiniz (isteğe bağlı)

Hassas veri (token, kişisel dosya yolu, çerez) paylaşmayın; gerekliyse örnekleri
anonimleştirin.

## Yanıt süreci

| Aşama | Hedef |
|---|---|
| Alındı bildirimi | 72 saat içinde |
| İlk değerlendirme | 7 gün içinde |
| Düzeltme / azaltma planı | 30 gün içinde (önem derecesine göre) |
| Kamuya açıklama | Düzeltme yayımlandıktan sonra, koordineli |

Süreç boyunca sizinle iletişimde kalır, sorularınızı yanıtlar ve düzeltmeyi test etmenizi
isteriz. Düzeltme hazır olmadan ayrıntıları kamuya açıklamamanızı rica ederiz. Bildiriminiz
kabul edilirse, izninizle sürüm notlarında size teşekkür ederiz.

Bu proje bir bug bounty programı sunmaz.

## Tehdit modeli

Hydra DL masaüstünde çalışır ve iki yerel HTTP yüzeyi açar. Güvenlik değerlendirmesinde
özellikle şu alanlar önemlidir:

### Yakalama sunucusu (`127.0.0.1:6712`)

- Varsayılan olarak yalnız geri döngü arayüzünü dinler.
- Kimlik doğrulaması yoktur ve CORS `*` olarak açıktır; tarayıcı eklentisinin
  çalışabilmesi için bu böyledir. Sonuç: **yerel makinede açık olan herhangi bir web
  sayfası** `/add` çağrısıyla indirme başlatabilir.
- Ayarlardan kapatılabilir (`Settings.captureEnabled`). Güvenilmeyen makinelerde veya
  güvenilmeyen sayfalar açarken kapatılması önerilir.
- Bu sunucuya internetten erişilemez; yönlendirme/port açma yapılmamalıdır.

### Yerel ağ web sunucusu (`<lan-ip>:6790`)

- Telefon/tablet arayüzü için LAN'a açılır.
- `Settings.webToken` boş değilse `?token=` parametresi veya `X-Hydra-Token` başlığı
  zorunludur. Token boş bırakılırsa aynı ağdaki herkes arayüze erişebilir.
- Varsayılan olarak kapalı tutulması önerilir; yalnız güvenilen ağlarda açın.

### İndirme girdileri

- URL şeması yalnız `http`/`https` (ve torrent için `magnet`) kabul edilir.
- Dosya adları Windows yasak karakterlerinden arındırılır ve hedef klasör dışına çıkacak
  yollar reddedilir; yine de hatalı bir yol birleştirmesi güvenlik açığı sayılır.
- yt-dlp ve torrent motoru dış süreç/kütüphane olarak çalışır; araç argümanları
  kullanıcı girdisinden **kabuk üzerinden** geçirilmemelidir.

### Kapsam dışı

- Kullanıcının kendi makinesinde yönetici yetkisiyle yaptığı değişiklikler
- Üçüncü taraf sitelerin (YouTube vb.) kendi güvenlik sorunları
- Kullanıcının indirdiği dosyaların içeriği ve güvenilirliği
- Desteklenmeyen platformlarda (Linux/macOS deneysel) görülen kararsızlıklar

## Güvenli kullanım önerileri

- Uygulamayı yalnızca güvendiğiniz kaynaklardan derleyin veya indirin; resmî bir ikili
  dağıtım kanalı henüz yoktur.
- Yakalama sunucusunu kullanmadığınızda kapatın.
- Yerel ağ sunucusunu token olmadan açık bırakmayın.
- `yt-dlp`'yi resmî kaynağından güncel tutun.
- Şüpheli bir davranış gördüğünüzde uygulamayı kapatın ve yukarıdaki kanaldan bildirin.
