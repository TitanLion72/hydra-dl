# Sık Sorulan Sorular

## Genel

### Hydra DL ücretsiz mi?

Evet. Proje [AGPL-3.0-or-later](../LICENSE) ile lisanslanmıştır; ücretsiz kullanabilir,
inceleyebilir, değiştirebilir ve dağıtabilirsiniz. Ticari kullanım da serbesttir; tek koşul,
dağıtılan veya ağ üzerinden sunulan sürümün kaynak kodunun da AGPL altında paylaşılmasıdır.

### IDM'nin yerini tutar mı?

Temel akışta (parçalı indirme, devam, kuyruk, tarayıcıdan yakalama) evet. Ancak proje gençtir:
çerez aktarımı, proxy ve otomatik güncelleme henüz yok. Yetişkin/uzun süredir kullanılan IDM
alışkanlıklarınız varsa farkları göz önünde bulundurun.

### Hangi platformlar destekleniyor?

Windows birinci sınıf hedeftir (Windows 10/11, WebView2). Linux ve macOS derlenebilir ve
temel akış çalışır, fakat paketleme ve kenar durumlar deneyseldir.

### Verilerim nereye kaydediliyor?

`%APPDATA%\com.hydradl.app\` altında:

- `downloads.json` — indirme listesi ve ayarlar
- `parts\<id>\` — yarım parçalar ve geçici dosyalar
- `bin\yt-dlp.exe` — isterseniz buraya koyduğunuz yt-dlp

İndirilen dosyalar seçtiğiniz hedef klasöre gider.

## Kurulum ve derleme

### `npm.cmd run tauri dev` açılmıyor

- Vite portu **1420** `strictPort` ile sabittir; başka bir süreç kullanıyorsa kapatın.
- `tauri-dev.log` dosyasına bakın.
- İlk derleme uzun sürer; sabırlı olun.
- Windows'ta WebView2 Runtime kurulu mu kontrol edin.

### `npm` çalışmıyor, `npm.ps1` engelli

PowerShell execution policy nedeniyle olur. `npm.cmd` kullanın:

```powershell
npm.cmd install
npm.cmd run tauri dev
```

### Rust sürümüm eski

`rustup update stable` çalıştırın. Proje Rust 1.77+ ve edition 2021 kullanır.

### Linux/macOS'ta sistem bağımlılıkları eksik

Tauri v2'nin platform bağımlılıklarını (webkit2gtk, libappindicator vb.) kurmanız gerekir:
<https://tauri.app/start/prerequisites/>

### Derleme `librqbit` veya `sha1` hatası veriyor

Torrent motoru `librqbit` bir SHA-1 sağlayıcısı ister. Derleme bu adımda düşerse önce
`main` dalını güncelleyin; sorun sürüyorsa hata çıktısıyla issue açın.

## Kullanım

### İndirme neden tek bağlantıya düştü?

Sunucu `Range` isteklerini desteklemiyor veya `Accept-Ranges` belirtmiyor olabilir. Hydra DL
bu durumda otomatik olarak tek bağlantıyla indirir. Dosya 1 MiB'den küçükse de bölme
yapılmaz.

### Uygulamayı kapattım, indirme devam eder mi?

Evet, `autoResume` açıksa yarım parçalar korunur ve uygulama açıldığında devam eder.
**Duraklatmak** parçaları saklar; **iptal etmek** parçaları siler.

### Hız limiti neden tam tutmuyor?

Limit tüm bağlantıların paylaştığı kaba bir global bütçedir; bağlantı başına adil paylaşım
yapmaz. Ayrıca değişiklik aktif indirmelere bir sonraki parça okumasında yansır.

### YouTube / Instagram indirmiyor

- `yt-dlp` kurulu mu? Ayarlar ekranında durumu görünür.
- `yt-dlp -U` ile güncelleyin; siteler sık değişir.
- Bazı içerikler giriş/çerez ister; çerez aktarımı henüz yok.
- DRM korumalı içerik **desteklenmez** ve desteklenmeyecektir.

### Torrent indirmesi başlamıyor

- `magnet:` bağlantısının eksiksiz kopyalandığından emin olun.
- Güvenlik duvarı/virüs programı P2P trafiğini engelliyor olabilir.
- Sağlıklı seeder yoksa indirme çok yavaş ilerler veya hiç başlamaz.

### Dosya adı bozuldu / Türkçe karakterler kayboldu

Dosya adları Windows yasak karakterlerinden arındırılır ve 180 karaktere kırpılır. Sunucu
`Content-Disposition` başlığında yanlış kodlama gönderiyorsa ad yanlış çözülebilir; URL'den
veya elle ad girerek düzeltebilirsiniz.

## Tarayıcı eklentisi

### “Hydra DL'ye ulaşılamadı” diyor

- Masaüstü uygulaması açık mı?
- Ayarlarda yakalama sunucusu açık mı, port eklentiyle aynı mı (varsayılan `6712`)?
- Port başka bir uygulama tarafından kullanılıyorsa ikisinden birini değiştirin.

### Eklenti hangi verileri topluyor?

Hiçbiri. Yalnız `http://127.0.0.1:<port>` adresine istek atar; port ve tercihler
`chrome.storage.sync` içinde tutulur. Uzak sunucuya veri göndermez.

### Avcı modu nedir?

Sayfadaki videoyu veya dosyayı fareyle seçip tek tıkla Hydra DL'ye göndermenizi sağlar.
Adres, sayfanın `Referer` bilgisiyle birlikte iletilir. İstemiyorsanız paneli kapatabilirsiniz.

### Eklenti mağazada mı?

Hayır. Şimdilik `extension/` klasörünü “paketlenmemiş öğe” olarak yüklemeniz gerekir.

## Telefon / yerel ağ

### Telefondan bağlanamıyorum

- Bilgisayar ve telefon aynı Wi-Fi ağında olmalı.
- Windows Güvenlik Duvarı 6790 portuna yerel ağ erişimine izin vermeli.
- Misafir ağı / istemci izolasyonu açıksa cihazlar birbirini görmez.
- Adresi tarayıcıya tam yazın: `http://<bilgisayar-ip>:6790/?token=...`

### Token zorunlu mu?

`Settings.webToken` boşsa hayır; ama aynı ağdaki herkes erişebilir. Güvenmediğiniz ağlarda
token belirleyin veya sunucuyu kapatın.

### QR kod nerede?

Yerel ağ sunucusu açıkken masaüstü ayarlarında gösterilir; telefon kamerasıyla okutarak
adres + token ile açabilirsiniz.

## Gizlilik ve güvenlik

### Yakalama sunucusu güvenli mi?

Yalnız `127.0.0.1` dinler, ama CORS açıktır ve token istemez. Yerel makinede açık herhangi bir
sayfa indirme ekleyebilir. Kullanmadığınızda kapatın. Ayrıntılar: [`SECURITY.md`](../SECURITY.md).

### Telemetri var mı?

Hayır. Uygulama hiçbir analiz/telemetri servisine veri göndermez.

## Katkı

### Nereden başlamalıyım?

[`CONTRIBUTING.md`](../CONTRIBUTING.md) ve `PLAN.md` dosyalarına bakın. `good first issue`
etiketli issue'lar başlangıç için uygundur. Büyük değişikliklerden önce issue açın.

### Hata bildirirken neleri eklemeliyim?

Sürüm, işletim sistemi, adımlar, beklenen/gerçek davranış, `tauri-dev.log` ilgili kısmı ve
mümkünse ekran görüntüsü. Kişisel bilgileri ve token'ları maskeleyin.

### Lisans değişikliği olacak mı?

Hayır. Proje AGPL-3.0-or-later olarak kalacaktır; katkılar da bu lisans altında kabul edilir.
