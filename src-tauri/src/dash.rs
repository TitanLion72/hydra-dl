//! DASH (`.mpd`) manifest çözümü — yt-dlp olmadan indirme için.
//!
//! Kapsam (bilinçli olarak dar, HLS ile aynı çizgide):
//! - `SegmentTemplate` (`$Number$` + `duration`/`timescale` **ve** `SegmentTimeline`),
//!   `SegmentList`, tek dosyalı `SegmentBase` (BaseURL) desteklenir.
//! - `BaseURL` zinciri (MPD → Period → AdaptationSet → Representation) çözülür.
//! - `ContentProtection` görülürse **reddedilir**: DRM kırmıyoruz.
//! - Ses ve video ayrı `AdaptationSet`'lerdeyse ikisi de indirilir (ayrı dosyalar);
//!   birleştirme için ffmpeg/yt-dlp gerekir, o yüzden kullanıcıya açıkça söylenir.
//!
//! Bu modül **saf**tır (ağ yok): MPD metni → indirilecek iş listesi.

use quick_xml::events::Event;
use quick_xml::Reader;

/// Byte-range: (başlangıç, uzunluk). Uzunluk `None` ise sonuna kadar.
pub type ByteRange = (u64, Option<u64>);
/// Şablon/URL + isteğe bağlı byte-range.
pub type RangedUrl = (String, Option<ByteRange>);

/// Bir akışın türü.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Video,
    Audio,
}

/// Tek bir segment (mutlak adres).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashSegment {
    pub url: String,
    /// SegmentBase/SegmentList byte-range (başlangıç, uzunluk).
    pub range: Option<ByteRange>,
}

/// İndirilecek tek akış (video veya ses).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashTrack {
    pub kind: TrackKind,
    pub id: String,
    pub bandwidth: u64,
    pub codecs: Option<String>,
    pub width: Option<u64>,
    pub height: Option<u64>,
    /// `mimeType` (ör. `video/mp4`).
    pub mime: String,
    /// fMP4 başlangıç parçası (`initialization`).
    pub init: Option<DashSegment>,
    pub segments: Vec<DashSegment>,
}

#[derive(Debug, Clone, Default)]
struct SegmentTemplateInfo {
    media: Option<String>,
    initialization: Option<String>,
    /// `$Number$` başlangıcı.
    start_number: Option<u64>,
    /// `duration` / `timescale` → segment süresi (saniye cinsinden değil, tik).
    duration: Option<u64>,
    timescale: Option<u64>,
    /// `SegmentTimeline` girişleri: (t, d, r) üçlüleri.
    timeline: Vec<(Option<u64>, u64, i64)>,
    presentation_time_offset: Option<u64>,
}

#[derive(Debug, Clone, Default)]
struct SegmentListInfo {
    #[allow(dead_code)]
    duration: Option<u64>,
    #[allow(dead_code)]
    timescale: Option<u64>,
    initialization: Option<RangedUrl>,
    /// (media, range)
    urls: Vec<RangedUrl>,
}

#[derive(Debug, Clone, Default)]
struct SegmentBaseInfo {
    initialization: Option<RangedUrl>,
    /// `indexRange` — şu an yalnız bilgi; tam dosya indirilir.
    #[allow(dead_code)]
    index_range: Option<ByteRange>,
}

#[derive(Debug, Clone, Default)]
struct Representation {
    id: String,
    bandwidth: u64,
    codecs: Option<String>,
    width: Option<u64>,
    height: Option<u64>,
    mime: Option<String>,
    base_url: Option<String>,
    template: Option<SegmentTemplateInfo>,
    list: Option<SegmentListInfo>,
    base: Option<SegmentBaseInfo>,
}

#[derive(Debug, Clone, Default)]
struct AdaptationSet {
    mime: Option<String>,
    /// `contentType="video"` / `"audio"` ya da `mimeType` önekinden türetilir.
    kind: Option<TrackKind>,
    base_url: Option<String>,
    template: Option<SegmentTemplateInfo>,
    list: Option<SegmentListInfo>,
    base: Option<SegmentBaseInfo>,
    representations: Vec<Representation>,
}

#[derive(Debug, Clone, Default)]
struct Period {
    /// Period süresi (saniye) — `SegmentTemplate@duration` ile segment sayısı için.
    duration_secs: Option<f64>,
    base_url: Option<String>,
    adaptations: Vec<AdaptationSet>,
}

/// Manifest özeti: indirilecek akışlar.
#[derive(Debug, Clone, Default)]
pub struct DashManifest {
    pub tracks: Vec<DashTrack>,
}

/// `PT1H2M3.5S` biçimini saniyeye çevirir.
pub fn parse_duration(text: &str) -> Option<f64> {
    let text = text.trim();
    let rest = text.strip_prefix("PT")?;
    let mut total = 0.0;
    let mut number = String::new();
    for ch in rest.chars() {
        match ch {
            '0'..='9' | '.' => number.push(ch),
            'H' => {
                total += number.parse::<f64>().ok()? * 3600.0;
                number.clear();
            }
            'M' => {
                total += number.parse::<f64>().ok()? * 60.0;
                number.clear();
            }
            'S' => {
                total += number.parse::<f64>().ok()?;
                number.clear();
            }
            _ => return None,
        }
    }
    Some(total)
}

/// `bytes=0-1023` biçimini (başlangıç, uzunluk) çevirir.
fn parse_range(text: &str) -> Option<ByteRange> {
    let rest = text.trim().strip_prefix("bytes=")?;
    let (start, end) = rest.split_once('-')?;
    let start = start.trim().parse::<u64>().ok()?;
    let length = end
        .trim()
        .parse::<u64>()
        .ok()
        .map(|end| end.saturating_sub(start) + 1);
    Some((start, length))
}

fn attr_value(event: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Option<String> {
    for attribute in event.attributes().flatten() {
        if attribute.key.as_ref() == name {
            return attribute
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .ok()
                .map(|value| value.into_owned());
        }
    }
    None
}

fn attr_u64(event: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Option<u64> {
    attr_value(event, name).and_then(|value| value.trim().parse::<u64>().ok())
}

/// `$RepresentationID$`, `$Number%05d$`, `$Time$`, `$Bandwidth$` yerine koyar.
pub fn expand_template(
    template: &str,
    representation_id: &str,
    bandwidth: u64,
    number: u64,
    time: u64,
) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(start) = rest.find('$') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('$') else {
            out.push('$');
            out.push_str(after);
            return out;
        };
        let token = &after[..end];
        let (name, width) = match token.split_once('%') {
            Some((name, spec)) => {
                // "%05d" → genişlik 5, sıfır dolgu
                let digits: String = spec.chars().filter(|c| c.is_ascii_digit()).collect();
                (name, digits.parse::<usize>().ok())
            }
            None => (token, None),
        };
        let value = match name {
            "RepresentationID" => representation_id.to_string(),
            "Number" => number.to_string(),
            "Time" => time.to_string(),
            "Bandwidth" => bandwidth.to_string(),
            _ => String::new(),
        };
        match width {
            Some(width) => out.push_str(&format!("{value:0>width$}")),
            None => out.push_str(&value),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Göreli adresi tabana göre çözer.
pub fn resolve(base: &str, reference: &str) -> Option<String> {
    if reference.trim().is_empty() {
        return None;
    }
    let base = url::Url::parse(base).ok()?;
    base.join(reference.trim()).ok().map(|url| url.to_string())
}

/// MPD metnini çözer. `mpd_url` göreli adreslerin tabanıdır.
pub fn parse_mpd(xml: &str, mpd_url: &str) -> Result<DashManifest, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut mpd_duration = None;
    let mut mpd_base: Option<String> = None;
    let mut periods: Vec<Period> = Vec::new();
    let mut current_period: Option<Period> = None;
    let mut current_adaptation: Option<AdaptationSet> = None;
    let mut current_representation: Option<Representation> = None;
    let mut drm = false;

    // BaseURL metni için tampon: hangi seviyede olduğumuzu `target` tutar.
    let mut base_url_target: Option<u8> = None;
    let mut base_url_text = String::new();
    // SegmentTimeline içindeki <S> öğeleri hangi template'e yazılacak?
    let mut timeline_target: Option<u8> = None;

    loop {
        let event = reader
            .read_event()
            .map_err(|e| format!("MPD çözümlenemedi: {e}"))?;
        match event {
            Event::Start(ref start) | Event::Empty(ref start) => {
                let empty = matches!(event, Event::Empty(_));
                match start.name().as_ref() {
                    b"MPD" => {
                        mpd_duration = attr_value(start, b"mediaPresentationDuration")
                            .and_then(|text| parse_duration(&text));
                    }
                    b"Period" => {
                        current_period = Some(Period {
                            duration_secs: attr_value(start, b"duration")
                                .and_then(|text| parse_duration(&text)),
                            base_url: None,
                            adaptations: Vec::new(),
                        });
                    }
                    b"BaseURL" => {
                        base_url_target = Some(
                            match (
                                current_representation.is_some(),
                                current_adaptation.is_some(),
                                current_period.is_some(),
                            ) {
                                (true, _, _) => 4,
                                (_, true, _) => 3,
                                (_, _, true) => 2,
                                _ => 1,
                            },
                        );
                        base_url_text.clear();
                    }
                    b"AdaptationSet" => {
                        let mime = attr_value(start, b"mimeType");
                        let content_type = attr_value(start, b"contentType");
                        let kind = match content_type.as_deref() {
                            Some("video") => Some(TrackKind::Video),
                            Some("audio") => Some(TrackKind::Audio),
                            _ => mime.as_deref().map(|mime| {
                                if mime.starts_with("video") {
                                    TrackKind::Video
                                } else if mime.starts_with("audio") {
                                    TrackKind::Audio
                                } else {
                                    TrackKind::Video
                                }
                            }),
                        };
                        current_adaptation = Some(AdaptationSet {
                            mime,
                            kind,
                            ..Default::default()
                        });
                    }
                    b"Representation" => {
                        current_representation = Some(Representation {
                            id: attr_value(start, b"id").unwrap_or_default(),
                            bandwidth: attr_u64(start, b"bandwidth").unwrap_or(0),
                            codecs: attr_value(start, b"codecs"),
                            width: attr_u64(start, b"width"),
                            height: attr_u64(start, b"height"),
                            mime: attr_value(start, b"mimeType"),
                            ..Default::default()
                        });
                        if empty {
                            // <Representation ... /> — hemen kapat
                            if let (Some(adaptation), Some(rep)) =
                                (current_adaptation.as_mut(), current_representation.take())
                            {
                                adaptation.representations.push(rep);
                            }
                        }
                    }
                    b"SegmentTemplate" => {
                        let template = SegmentTemplateInfo {
                            media: attr_value(start, b"media"),
                            initialization: attr_value(start, b"initialization"),
                            start_number: attr_u64(start, b"startNumber"),
                            duration: attr_u64(start, b"duration"),
                            timescale: attr_u64(start, b"timescale"),
                            timeline: Vec::new(),
                            presentation_time_offset: attr_u64(start, b"presentationTimeOffset"),
                        };
                        if current_representation.is_some() {
                            if let Some(rep) = current_representation.as_mut() {
                                rep.template = Some(template);
                            }
                        } else if let Some(adaptation) = current_adaptation.as_mut() {
                            adaptation.template = Some(template);
                        }
                        timeline_target = Some(if current_representation.is_some() {
                            4
                        } else {
                            3
                        });
                    }
                    b"SegmentList" => {
                        let list = SegmentListInfo {
                            duration: attr_u64(start, b"duration"),
                            timescale: attr_u64(start, b"timescale"),
                            ..Default::default()
                        };
                        if let Some(rep) = current_representation.as_mut() {
                            rep.list = Some(list);
                        } else if let Some(adaptation) = current_adaptation.as_mut() {
                            adaptation.list = Some(list);
                        }
                    }
                    b"SegmentBase" => {
                        let info = SegmentBaseInfo {
                            index_range: attr_value(start, b"indexRange")
                                .and_then(|text| parse_range(&text)),
                            ..Default::default()
                        };
                        if let Some(rep) = current_representation.as_mut() {
                            rep.base = Some(info);
                        } else if let Some(adaptation) = current_adaptation.as_mut() {
                            adaptation.base = Some(info);
                        }
                    }
                    b"Initialization" => {
                        let source = attr_value(start, b"sourceURL").unwrap_or_default();
                        let range = attr_value(start, b"range").and_then(|text| parse_range(&text));
                        if let Some(rep) = current_representation.as_mut() {
                            if let Some(list) = rep.list.as_mut() {
                                list.initialization = Some((source, range));
                            } else if let Some(base) = rep.base.as_mut() {
                                base.initialization = Some((source, range));
                            }
                        } else if let Some(adaptation) = current_adaptation.as_mut() {
                            if let Some(list) = adaptation.list.as_mut() {
                                list.initialization = Some((source, range));
                            } else if let Some(base) = adaptation.base.as_mut() {
                                base.initialization = Some((source, range));
                            }
                        }
                    }
                    b"SegmentURL" => {
                        let media = attr_value(start, b"media").unwrap_or_default();
                        let range =
                            attr_value(start, b"mediaRange").and_then(|text| parse_range(&text));
                        if let Some(rep) = current_representation.as_mut() {
                            if let Some(list) = rep.list.as_mut() {
                                list.urls.push((media, range));
                            }
                        } else if let Some(adaptation) = current_adaptation.as_mut() {
                            if let Some(list) = adaptation.list.as_mut() {
                                list.urls.push((media, range));
                            }
                        }
                    }
                    b"S" => {
                        // SegmentTimeline girişi
                        let time = attr_u64(start, b"t");
                        let duration = attr_u64(start, b"d");
                        let repeat = attr_value(start, b"r")
                            .and_then(|text| text.trim().parse::<i64>().ok())
                            .unwrap_or(0);
                        if let (Some(duration), Some(target)) = (duration, timeline_target) {
                            let entry = (time, duration, repeat);
                            if target == 4 {
                                if let Some(rep) = current_representation.as_mut() {
                                    if let Some(template) = rep.template.as_mut() {
                                        template.timeline.push(entry);
                                    }
                                }
                            } else if let Some(adaptation) = current_adaptation.as_mut() {
                                if let Some(template) = adaptation.template.as_mut() {
                                    template.timeline.push(entry);
                                }
                            }
                        }
                    }
                    b"ContentProtection" => {
                        drm = true;
                    }
                    _ => {}
                }
            }
            Event::Text(ref text) => {
                if base_url_target.is_some() {
                    base_url_text.push_str(&text.xml10_content().unwrap_or_default());
                }
            }
            // quick-xml 0.38+ emits entity references (`&amp;`, `&#38;`) as
            // separate events instead of unescaping them inside Text.
            Event::GeneralRef(ref reference) => {
                if base_url_target.is_some() {
                    if let Ok(Some(ch)) = reference.resolve_char_ref() {
                        base_url_text.push(ch);
                    } else if let Ok(name) = reference.decode() {
                        match name.as_ref() {
                            "amp" => base_url_text.push('&'),
                            "lt" => base_url_text.push('<'),
                            "gt" => base_url_text.push('>'),
                            "quot" => base_url_text.push('"'),
                            "apos" => base_url_text.push('\''),
                            _ => {}
                        }
                    }
                }
            }
            Event::End(ref end) => match end.name().as_ref() {
                b"BaseURL" => {
                    if let Some(target) = base_url_target.take() {
                        let value = base_url_text.trim().to_string();
                        let value = if value.is_empty() { None } else { Some(value) };
                        match target {
                            4 => {
                                if let Some(rep) = current_representation.as_mut() {
                                    rep.base_url = value;
                                }
                            }
                            3 => {
                                if let Some(adaptation) = current_adaptation.as_mut() {
                                    adaptation.base_url = value;
                                }
                            }
                            2 => {
                                if let Some(period) = current_period.as_mut() {
                                    period.base_url = value;
                                }
                            }
                            _ => mpd_base = value,
                        }
                    }
                    base_url_text.clear();
                }
                b"Representation" => {
                    if let (Some(adaptation), Some(rep)) =
                        (current_adaptation.as_mut(), current_representation.take())
                    {
                        adaptation.representations.push(rep);
                    }
                }
                b"AdaptationSet" => {
                    if let (Some(period), Some(adaptation)) =
                        (current_period.as_mut(), current_adaptation.take())
                    {
                        period.adaptations.push(adaptation);
                    }
                }
                b"Period" => {
                    if let Some(period) = current_period.take() {
                        periods.push(period);
                    }
                }
                b"SegmentTimeline" => {
                    timeline_target = None;
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }

    if drm {
        return Err(
            "Bu MPD DRM korumalı (ContentProtection). Hydra DL DRM'lı içerik indirmez.".into(),
        );
    }
    if periods.is_empty() {
        return Err("MPD'de Period bulunamadı.".into());
    }

    // BaseURL zinciri (MPD → Period → AdaptationSet → Representation)
    let mpd_root =
        resolve(mpd_url, mpd_base.as_deref().unwrap_or("")).unwrap_or_else(|| mpd_url.to_string());

    let mut tracks = Vec::new();
    for period in &periods {
        let period_root = period
            .base_url
            .as_deref()
            .and_then(|value| resolve(&mpd_root, value))
            .unwrap_or_else(|| mpd_root.clone());
        let period_duration = period.duration_secs.or(mpd_duration);

        for adaptation in &period.adaptations {
            let adaptation_root = adaptation
                .base_url
                .as_deref()
                .and_then(|value| resolve(&period_root, value))
                .unwrap_or_else(|| period_root.clone());
            let kind = adaptation.kind.unwrap_or(TrackKind::Video);
            let mime = adaptation.mime.clone().unwrap_or_else(|| match kind {
                TrackKind::Video => "video/mp4".into(),
                TrackKind::Audio => "audio/mp4".into(),
            });

            for representation in &adaptation.representations {
                let rep_root = representation
                    .base_url
                    .as_deref()
                    .and_then(|value| resolve(&adaptation_root, value))
                    .unwrap_or_else(|| adaptation_root.clone());

                // SegmentTemplate temsilciden, yoksa AdaptationSet'ten miras alınır.
                let template = representation
                    .template
                    .clone()
                    .or_else(|| adaptation.template.clone());
                let list = representation
                    .list
                    .clone()
                    .or_else(|| adaptation.list.clone());
                let base = representation
                    .base
                    .clone()
                    .or_else(|| adaptation.base.clone());

                let (init, segments) = build_segments(
                    &rep_root,
                    representation,
                    template.as_ref(),
                    list.as_ref(),
                    base.as_ref(),
                    period_duration,
                )?;
                if segments.is_empty() && init.is_none() {
                    continue;
                }
                tracks.push(DashTrack {
                    kind,
                    id: representation.id.clone(),
                    bandwidth: representation.bandwidth,
                    codecs: representation.codecs.clone(),
                    width: representation.width,
                    height: representation.height,
                    mime: representation.mime.clone().unwrap_or_else(|| mime.clone()),
                    init,
                    segments,
                });
            }
        }
    }

    if tracks.is_empty() {
        return Err("MPD'de indirilebilir akış bulunamadı.".into());
    }
    Ok(DashManifest { tracks })
}

/// Bir representation'ın segment listesini üretir.
fn build_segments(
    rep_root: &str,
    representation: &Representation,
    template: Option<&SegmentTemplateInfo>,
    list: Option<&SegmentListInfo>,
    base: Option<&SegmentBaseInfo>,
    period_duration: Option<f64>,
) -> Result<(Option<DashSegment>, Vec<DashSegment>), String> {
    // 1) SegmentTemplate (yaygın durum)
    if let Some(template) = template {
        let Some(media) = template.media.as_deref() else {
            return Err("SegmentTemplate'te `media` yok.".into());
        };
        let timescale = template.timescale.unwrap_or(1).max(1);
        let start_number = template.start_number.unwrap_or(1);
        let mut segments = Vec::new();

        if !template.timeline.is_empty() {
            // SegmentTimeline: $Time$ (ya da $Number$) ile üretilir.
            let mut time = template.presentation_time_offset.unwrap_or(0);
            let mut number = start_number;
            for (t, d, repeat) in &template.timeline {
                if let Some(t) = t {
                    time = *t;
                }
                let repeats = if *repeat < 0 {
                    // -1: Period sonuna kadar sürer; süre bilinmiyorsa tek segment.
                    match period_duration {
                        Some(secs) => {
                            let total = (secs * timescale as f64) as u64;
                            total.saturating_sub(time) / d.max(&1) - 1
                        }
                        None => 0,
                    }
                } else {
                    *repeat as u64
                };
                for _ in 0..=repeats {
                    let url = expand_template(
                        media,
                        &representation.id,
                        representation.bandwidth,
                        number,
                        time,
                    );
                    segments.push(DashSegment {
                        url: resolve(rep_root, &url)
                            .ok_or_else(|| format!("segment adresi çözülemedi: {url}"))?,
                        range: None,
                    });
                    time += d;
                    number += 1;
                }
            }
        } else if let (Some(duration), Some(period_secs)) = (template.duration, period_duration) {
            // duration/timescale ile segment sayısı
            let total_ticks = (period_secs * timescale as f64).round() as u64;
            let count = total_ticks.div_ceil(duration.max(1));
            for index in 0..count {
                let number = start_number + index;
                let time = index * duration;
                let url = expand_template(
                    media,
                    &representation.id,
                    representation.bandwidth,
                    number,
                    time,
                );
                segments.push(DashSegment {
                    url: resolve(rep_root, &url)
                        .ok_or_else(|| format!("segment adresi çözülemedi: {url}"))?,
                    range: None,
                });
            }
        } else if template.duration.is_some() {
            // Segment sayısı toplam süreye bağlı; süre yoksa tahmin etmiyoruz.
            return Err(
                "MPD'de toplam süre yok (mediaPresentationDuration/Period@duration); segment sayısı belirlenemiyor."
                    .into(),
            );
        }

        let init = template.initialization.as_deref().and_then(|init| {
            let url = expand_template(
                init,
                &representation.id,
                representation.bandwidth,
                start_number,
                0,
            );
            resolve(rep_root, &url).map(|url| DashSegment { url, range: None })
        });
        return Ok((init, segments));
    }

    // 2) SegmentList
    if let Some(list) = list {
        let segments = list
            .urls
            .iter()
            .filter_map(|(media, range)| {
                resolve(rep_root, media).map(|url| DashSegment { url, range: *range })
            })
            .collect::<Vec<_>>();
        let init = list.initialization.as_ref().and_then(|(source, range)| {
            resolve(rep_root, source).map(|url| DashSegment { url, range: *range })
        });
        return Ok((init, segments));
    }

    // 3) Tek dosya (SegmentBase ya da yalnız BaseURL): tüm dosya indirilir.
    let init = base.and_then(|base| {
        base.initialization.as_ref().and_then(|(source, range)| {
            resolve(rep_root, source).map(|url| DashSegment { url, range: *range })
        })
    });
    Ok((
        init,
        vec![DashSegment {
            url: rep_root.to_string(),
            range: None,
        }],
    ))
}

/// En iyi video ve ses akışını seçer (en yüksek bant genişliği).
pub fn pick_tracks(manifest: &DashManifest) -> (Option<DashTrack>, Option<DashTrack>) {
    let best = |kind: TrackKind| {
        manifest
            .tracks
            .iter()
            .filter(|track| track.kind == kind)
            .max_by_key(|track| track.bandwidth)
            .cloned()
    };
    (best(TrackKind::Video), best(TrackKind::Audio))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATE_MPD: &str = r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" mediaPresentationDuration="PT30S" type="static">
  <Period>
    <AdaptationSet mimeType="video/mp4">
      <SegmentTemplate media="seg-$Number$.m4s" initialization="init.mp4"
                       startNumber="1" duration="2" timescale="1"/>
      <Representation id="v1" bandwidth="500000" width="640" height="360" codecs="avc1.4d401e"/>
      <Representation id="v2" bandwidth="3000000" width="1920" height="1080" codecs="avc1.640028"/>
    </AdaptationSet>
    <AdaptationSet mimeType="audio/mp4">
      <SegmentTemplate media="a-$Number$.m4s" initialization="a-init.mp4"
                       startNumber="1" duration="2" timescale="1"/>
      <Representation id="a1" bandwidth="128000" codecs="mp4a.40.2"/>
    </AdaptationSet>
  </Period>
</MPD>"#;

    #[test]
    fn sure_bicimi_cozulur() {
        assert_eq!(parse_duration("PT30S"), Some(30.0));
        assert_eq!(parse_duration("PT1H2M3.5S"), Some(3723.5));
        assert_eq!(parse_duration("PT0S"), Some(0.0));
        assert_eq!(parse_duration("30S"), None, "PT oneki olmalı");
    }

    #[test]
    fn sablon_yer_tutuculari_doldurulur() {
        assert_eq!(
            expand_template("seg-$Number%05d$.m4s", "v1", 1000, 7, 0),
            "seg-00007.m4s"
        );
        assert_eq!(
            expand_template("$RepresentationID$/seg-$Time$.m4s", "v2", 2000, 1, 1234),
            "v2/seg-1234.m4s"
        );
        assert_eq!(
            expand_template("$Bandwidth$/x.m4s", "v1", 500000, 1, 0),
            "500000/x.m4s"
        );
        // Bilinmeyen yer tutucu boş bırakılır (çökmez).
        assert_eq!(expand_template("a$x$b", "v1", 1, 1, 0), "ab");
    }

    #[test]
    fn sablon_segmentleri_uretilir() {
        let manifest =
            parse_mpd(TEMPLATE_MPD, "https://cdn.example.com/dash/manifest.mpd").unwrap();
        let (video, audio) = pick_tracks(&manifest);
        let video = video.expect("video akışı seçildi");
        let audio = audio.expect("ses akışı seçildi");

        assert_eq!(video.id, "v2", "en yüksek bant genişliği seçildi");
        assert_eq!(video.height, Some(1080));
        assert_eq!(video.kind, TrackKind::Video);
        assert_eq!(video.mime, "video/mp4");
        // 30 saniye / 2 saniye = 15 segment
        assert_eq!(video.segments.len(), 15);
        assert_eq!(
            video.segments[0].url,
            "https://cdn.example.com/dash/seg-1.m4s"
        );
        assert_eq!(
            video.segments[14].url,
            "https://cdn.example.com/dash/seg-15.m4s"
        );
        assert_eq!(
            video.init.as_ref().map(|seg| seg.url.as_str()),
            Some("https://cdn.example.com/dash/init.mp4")
        );
        assert_eq!(audio.kind, TrackKind::Audio);
        assert_eq!(audio.segments.len(), 15);
    }

    #[test]
    fn segment_timeline_cozulur() {
        let mpd = r#"<MPD mediaPresentationDuration="PT10S">
  <Period>
    <AdaptationSet mimeType="video/mp4">
      <SegmentTemplate media="s-$Time$.m4s" initialization="i.mp4" timescale="1000">
        <SegmentTimeline>
          <S t="0" d="2000" r="2"/>
          <S d="1000"/>
        </SegmentTimeline>
      </SegmentTemplate>
      <Representation id="v" bandwidth="1000"/>
    </AdaptationSet>
  </Period>
</MPD>"#;
        let manifest = parse_mpd(mpd, "https://x/y/manifest.mpd").unwrap();
        let (video, _) = pick_tracks(&manifest);
        let video = video.expect("video var");
        // r=2 → 3 tekrar (0,2,4 sn), sonra bir 1 sn'lik → toplam 4
        assert_eq!(video.segments.len(), 4);
        assert_eq!(video.segments[0].url, "https://x/y/s-0.m4s");
        assert_eq!(video.segments[1].url, "https://x/y/s-2000.m4s");
        assert_eq!(video.segments[2].url, "https://x/y/s-4000.m4s");
        assert_eq!(video.segments[3].url, "https://x/y/s-6000.m4s");
    }

    #[test]
    fn segment_list_cozulur() {
        let mpd = r#"<MPD mediaPresentationDuration="PT6S">
  <Period>
    <AdaptationSet mimeType="audio/mp4">
      <Representation id="a" bandwidth="64000">
        <SegmentList>
          <Initialization sourceURL="init.mp4"/>
          <SegmentURL media="s1.m4s"/>
          <SegmentURL media="s2.m4s"/>
          <SegmentURL media="s3.m4s"/>
        </SegmentList>
      </Representation>
    </AdaptationSet>
  </Period>
</MPD>"#;
        let manifest = parse_mpd(mpd, "https://x/y/manifest.mpd").unwrap();
        let (_, audio) = pick_tracks(&manifest);
        let audio = audio.expect("ses var");
        assert_eq!(audio.segments.len(), 3);
        assert_eq!(audio.segments[0].url, "https://x/y/s1.m4s");
        assert_eq!(
            audio.init.as_ref().map(|s| s.url.as_str()),
            Some("https://x/y/init.mp4")
        );
    }

    #[test]
    fn baseurl_zinciri_cozulur() {
        let mpd = r#"<MPD mediaPresentationDuration="PT4S">
  <BaseURL>https://cdn.example.com/kok/</BaseURL>
  <Period>
    <BaseURL>p1/</BaseURL>
    <AdaptationSet mimeType="video/mp4">
      <BaseURL>video/</BaseURL>
      <SegmentTemplate media="s-$Number$.m4s" initialization="init.mp4" duration="2" timescale="1"/>
      <Representation id="v" bandwidth="1000">
        <BaseURL>dusuk/</BaseURL>
      </Representation>
    </AdaptationSet>
  </Period>
</MPD>"#;
        let manifest = parse_mpd(mpd, "https://x/manifest.mpd").unwrap();
        let (video, _) = pick_tracks(&manifest);
        let video = video.expect("video var");
        assert_eq!(
            video.segments[0].url,
            "https://cdn.example.com/kok/p1/video/dusuk/s-1.m4s"
        );
        assert_eq!(
            video.init.as_ref().map(|s| s.url.as_str()),
            Some("https://cdn.example.com/kok/p1/video/dusuk/init.mp4")
        );
    }

    #[test]
    fn drm_reddedilir() {
        let mpd = r#"<MPD mediaPresentationDuration="PT4S">
  <Period>
    <AdaptationSet mimeType="video/mp4">
      <ContentProtection schemeIdUri="urn:uuid:edef8ba9-79d6-4ace-a3c8-27dcd51d21ed"/>
      <SegmentTemplate media="s-$Number$.m4s" duration="2" timescale="1"/>
      <Representation id="v" bandwidth="1000"/>
    </AdaptationSet>
  </Period>
</MPD>"#;
        let err = parse_mpd(mpd, "https://x/manifest.mpd").unwrap_err();
        assert!(err.contains("DRM"), "DRM mesajı beklenir: {err}");
    }

    #[test]
    fn toplam_sure_yoksa_acik_hata_verilir() {
        let mpd = r#"<MPD>
  <Period>
    <AdaptationSet mimeType="video/mp4">
      <SegmentTemplate media="s-$Number$.m4s" duration="2" timescale="1"/>
      <Representation id="v" bandwidth="1000"/>
    </AdaptationSet>
  </Period>
</MPD>"#;
        let err = parse_mpd(mpd, "https://x/manifest.mpd").unwrap_err();
        assert!(err.contains("toplam süre"), "açık hata mesajı: {err}");
    }

    #[test]
    fn byterange_ayristirilir() {
        assert_eq!(parse_range("bytes=0-1023"), Some((0, Some(1024))));
        assert_eq!(parse_range("bytes=2048-"), Some((2048, None)));
        assert_eq!(parse_range("0-10"), None, "bytes= öneki gerekli");
    }
}

#[cfg(test)]
mod entity_tests {
    use super::*;

    #[test]
    fn baseurl_entity_references_are_resolved() {
        let xml = r#"<?xml version="1.0"?><MPD mediaPresentationDuration="PT10S"><BaseURL>https://cdn.example.com/a/</BaseURL><Period><AdaptationSet mimeType="video/mp4"><Representation id="v" bandwidth="1000" width="10" height="10"><BaseURL>v.mp4?x=1&amp;y=&#50;</BaseURL></Representation></AdaptationSet></Period></MPD>"#;
        let manifest = parse_mpd(xml, "https://e.com/m.mpd").unwrap();
        let dbg = format!("{:?}", manifest);
        assert!(dbg.contains("https://cdn.example.com/a/v.mp4?x=1&y=2"), "{dbg}");
    }
}
