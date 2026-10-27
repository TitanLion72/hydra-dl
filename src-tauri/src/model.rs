//! Veri modeli ve saf yardimci hesaplar (ag yok, test edilebilir).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Tek parcanin altinda bolme yapmaya deger kabul edilen boyut.
pub const MIN_SEGMENT_SIZE: u64 = 1024 * 1024;
/// Ayni dosya icin ust sinir; sunuculari bogmayalim.
pub const MAX_CONNECTIONS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Queued,
    Downloading,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl Status {
    pub fn is_running(self) -> bool {
        matches!(self, Status::Downloading)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Dogrudan HTTP(S) dosya indirmesi (segmentli).
    Http,
    /// yt-dlp uzerinden medya indirmesi.
    Media,
    /// BitTorrent (magnet veya .torrent).
    Torrent,
}

impl Kind {
    /// Indirme sirasinda baglanti sayisi anlamli mi?
    #[allow(dead_code)] // torrent/medya ayrimi UI tarafinda kullaniliyor.
    pub fn is_http(self) -> bool {
        matches!(self, Kind::Http)
    }
}

/// Torrent suru istatistigi.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Swarm {
    #[serde(default)]
    pub peers: usize,
    #[serde(default)]
    pub seeders: usize,
}

/// Torrent icindeki tek dosya. `included=false` ise indirilmez.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentFile {
    pub index: usize,
    /// Torrent kokune gore yol, `/` ayirici ile.
    pub path: String,
    pub length: u64,
    #[serde(default = "default_true")]
    pub included: bool,
}

fn default_true() -> bool {
    true
}

/// Secili dosyalarin toplam boyutu. Hicbiri secili degilse `None`.
pub fn selected_total(files: &[TorrentFile]) -> Option<u64> {
    let mut any = false;
    let mut sum = 0u64;
    for file in files.iter().filter(|f| f.included) {
        any = true;
        sum = sum.saturating_add(file.length);
    }
    any.then_some(sum)
}

/// `files` listesine gore `included` bayraklarini gunceller.
/// Bos liste (hicbir dosya secilmedi) hatadir: torrent bosa inmez.
pub fn apply_selection(files: &mut [TorrentFile], selected: &[usize]) -> Result<(), String> {
    if selected.is_empty() {
        return Err("En az bir dosya seçmelisin".into());
    }
    for file in files.iter_mut() {
        file.included = selected.contains(&file.index);
    }
    if !files.iter().any(|f| f.included) {
        return Err("Seçilen dosya numaraları bu torrent'te yok".into());
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub index: usize,
    /// Ilk baytin mutlak ofseti (dahil).
    pub start: u64,
    /// Son baytin mutlak ofseti (dahil). `None` = boyut bilinmiyor, akis.
    pub end: Option<u64>,
    /// Bu segmentte simdiye kadar yazilan bayt.
    pub done: u64,
}

impl Segment {
    pub fn total_len(&self) -> Option<u64> {
        self.end.map(|end| end.saturating_sub(self.start) + 1)
    }

    pub fn remaining(&self) -> Option<u64> {
        self.total_len().map(|len| len.saturating_sub(self.done))
    }

    /// Bu segment icin istenecek `Range` basligi (bos segmentte `None`).
    pub fn range_header(&self) -> Option<String> {
        match (self.end, self.remaining()) {
            (Some(_), Some(0)) => None,
            (Some(end), _) => Some(format!("bytes={}-{}", self.start + self.done, end)),
            (None, _) if self.done > 0 => Some(format!("bytes={}-", self.start + self.done)),
            (None, _) => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    pub id: String,
    pub url: String,
    pub filename: String,
    pub dir: PathBuf,
    pub kind: Kind,
    pub status: Status,
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub segments: Vec<Segment>,
    /// Sunucu Range destekliyorsa duraklat/devam mumkun.
    pub resumable: bool,
    pub speed_bps: u64,
    /// 0..1; toplam boyut bilinmediginde yt-dlp yuzdesi.
    #[serde(default)]
    pub progress: Option<f64>,
    pub error: Option<String>,
    pub created_at: i64,
    pub finished_at: Option<i64>,
    pub connections: usize,
    /// Medya indirmesinde kaynak sayfa (referer) bilgisi.
    #[serde(default)]
    pub referer: Option<String>,
    #[serde(default)]
    pub mime: Option<String>,
    #[serde(default)]
    pub file_path: Option<PathBuf>,
    /// Medya indirmesinde secilen yt-dlp format kimligi.
    #[serde(default)]
    pub format: Option<String>,
    /// Torrent suru durumu.
    #[serde(default)]
    pub swarm: Option<Swarm>,
    /// Torrent icin bilgi ozeti (magnet) veya kaynak.
    #[serde(default)]
    pub source: Option<String>,
    /// Torrent dosya listesi ve secim durumu (coklu dosya torrentlerinde).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub torrent_files: Option<Vec<TorrentFile>>,
}

impl Download {
    #[allow(dead_code)] // UI ayni hesabi kendi tarafinda yapar; model API'si olarak duruyor.
    pub fn fraction(&self) -> f64 {
        if let Some(total) = self.total_bytes {
            if total > 0 {
                return (self.downloaded_bytes as f64 / total as f64).clamp(0.0, 1.0);
            }
        }
        self.progress.unwrap_or(0.0).clamp(0.0, 1.0)
    }

    #[allow(dead_code)]
    pub fn eta_secs(&self) -> Option<u64> {
        if self.status != Status::Downloading || self.speed_bps == 0 {
            return None;
        }
        let total = self.total_bytes?;
        let remaining = total.saturating_sub(self.downloaded_bytes);
        if remaining == 0 {
            return Some(0);
        }
        Some(remaining / self.speed_bps)
    }
}

/// Tum acik indirmelerin toplam soket butcesi. `connections` ayari dosya basina
/// ust sinirdir; ayni anda N indirme varsa butce paylasilir. Aksi halde
/// "10 indirme × 16 baglanti = 160 soket" olusup tek tek akislarin hizi da
/// toplam hiz da dusuyordu (kullanici bildirimi: "eszamanli indirmelerde hiz bolunuyor").
/// 64: 45 Mbps hatta 10 eszamanli indirme dosya basina 6 baglanti alir; sunucu
/// baglanti basina limit koydugunda tek dosya 3 baglantiya mahkum kalmasin.
pub const MAX_TOTAL_CONNECTIONS: usize = 64;

/// Ayni anda calisan indirmelere baglanti butcesini paylastirir.
///
/// Ornek: 1 indirme → 16, 4 → 16, 10 → 6.
pub fn fair_connections(connections: usize, active: usize) -> usize {
    let connections = connections.clamp(1, MAX_CONNECTIONS);
    let active = active.max(1);
    let share = (MAX_TOTAL_CONNECTIONS / active).max(1);
    connections.min(share).max(1)
}

/// Dosyayi `connections` parcaya boler. `end` dahildir, parcalar arasinda
/// bosluk veya cakisma olmaz; toplam uzunluk her zaman `total`a esittir.
pub fn plan_segments(total: Option<u64>, resumable: bool, connections: usize) -> Vec<Segment> {
    let Some(total) = total else {
        return vec![Segment {
            index: 0,
            start: 0,
            end: None,
            done: 0,
        }];
    };
    if total == 0 {
        return Vec::new();
    }

    let max_parts = if resumable {
        let by_size = total.div_ceil(MIN_SEGMENT_SIZE).max(1);
        by_size.min(connections.clamp(1, MAX_CONNECTIONS) as u64)
    } else {
        1
    };
    let parts = max_parts.max(1);
    let base = total / parts;
    let extra = total % parts;

    let mut segments = Vec::with_capacity(parts as usize);
    let mut cursor = 0u64;
    for index in 0..parts {
        let len = base + if index < extra { 1 } else { 0 };
        let start = cursor;
        let end = start + len - 1;
        segments.push(Segment {
            index: index as usize,
            start,
            end: Some(end),
            done: 0,
        });
        cursor = end + 1;
    }
    debug_assert_eq!(cursor, total);
    debug_assert_eq!(segments_len(&segments), Some(total));
    segments
}

/// Planin toplam uzunlugunu dogrular (testler ve guvenlik agi icin).
pub fn segments_len(segments: &[Segment]) -> Option<u64> {
    let mut sum = 0u64;
    for seg in segments {
        sum = sum.checked_add(seg.total_len()?)?;
    }
    Some(sum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bilinmeyen_boyut_tek_akis_olur() {
        let segs = plan_segments(None, true, 8);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].end, None);
    }

    #[test]
    fn range_desteklemeyen_sunucu_tek_parca() {
        let segs = plan_segments(Some(50 * 1024 * 1024), false, 8);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].start, 0);
        assert_eq!(segs[0].end, Some(50 * 1024 * 1024 - 1));
    }

    #[test]
    fn kucuk_dosya_tek_parca() {
        let segs = plan_segments(Some(200 * 1024), true, 8);
        assert_eq!(segs.len(), 1);
        assert_eq!(segments_len(&segs), Some(200 * 1024));
    }

    #[test]
    fn sifir_boyut_segment_uretmez() {
        assert!(plan_segments(Some(0), true, 8).is_empty());
    }

    #[test]
    fn parcalar_bosluksuz_ve_tam_ortuler() {
        for total in [
            1u64,
            1023,
            1024 * 1024,
            1024 * 1024 + 1,
            12_345_678,
            999_999_999,
        ] {
            for conns in [1usize, 2, 3, 7, 8, 16, 99] {
                let segs = plan_segments(Some(total), true, conns);
                assert_eq!(
                    segments_len(&segs),
                    Some(total),
                    "total={total} conns={conns}"
                );
                assert_eq!(segs[0].start, 0);
                assert_eq!(segs.last().unwrap().end, Some(total - 1));
                for pair in segs.windows(2) {
                    assert_eq!(pair[0].end.unwrap() + 1, pair[1].start);
                }
                assert!(segs.len() <= conns.clamp(1, MAX_CONNECTIONS));
            }
        }
    }

    #[test]
    fn baglanti_butcesi_eszamanli_indirmelere_paylastirilir() {
        // Kullanici bildirimi: eszamanli indirmelerde hiz bolunuyor.
        // Toplam soket sayisi MAX_TOTAL_CONNECTIONS'i asmamali.
        assert_eq!(fair_connections(16, 1), 16);
        assert_eq!(fair_connections(16, 2), 16);
        assert_eq!(fair_connections(16, 3), 16);
        assert_eq!(fair_connections(16, 4), 16);
        assert_eq!(fair_connections(16, 5), 12);
        assert_eq!(fair_connections(16, 10), 6);
        // Asiri istek de butceye uyar, ayar 1 ise tek baglanti kalir.
        assert_eq!(fair_connections(99, 4), 16);
        assert_eq!(fair_connections(1, 4), 1);
        assert_eq!(fair_connections(0, 0), 1);
        for active in 1..=10usize {
            assert!(
                fair_connections(16, active) * active <= MAX_TOTAL_CONNECTIONS,
                "active={active}"
            );
        }
    }

    #[test]
    fn range_header_dogru_hesaplanir() {
        let mut seg = Segment {
            index: 0,
            start: 1000,
            end: Some(1999),
            done: 0,
        };
        assert_eq!(seg.range_header().as_deref(), Some("bytes=1000-1999"));
        seg.done = 500;
        assert_eq!(seg.range_header().as_deref(), Some("bytes=1500-1999"));
        seg.done = 1000;
        assert_eq!(seg.range_header(), None);

        let open = Segment {
            index: 0,
            start: 0,
            end: None,
            done: 10,
        };
        assert_eq!(open.range_header().as_deref(), Some("bytes=10-"));
        assert_eq!(
            Segment {
                index: 0,
                start: 0,
                end: None,
                done: 0
            }
            .range_header(),
            None
        );
    }

    #[test]
    fn ilerleme_ve_eta() {
        let mut d = Download {
            id: "x".into(),
            url: "u".into(),
            filename: "f".into(),
            dir: PathBuf::from("."),
            kind: Kind::Http,
            status: Status::Downloading,
            total_bytes: Some(1000),
            downloaded_bytes: 400,
            segments: vec![],
            resumable: true,
            speed_bps: 100,
            progress: None,
            error: None,
            created_at: 0,
            finished_at: None,
            connections: 4,
            referer: None,
            mime: None,
            file_path: None,
            format: None,
            swarm: None,
            source: None,
            torrent_files: None,
        };
        assert!((d.fraction() - 0.4).abs() < 1e-9);
        assert_eq!(d.eta_secs(), Some(6));
        d.speed_bps = 0;
        assert_eq!(d.eta_secs(), None);
    }
}
