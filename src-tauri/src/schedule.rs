//! Zamanlanmis indirme penceresi (saf).
//!
//! Kullanici "yalniz 02:00-06:00 arasi indir" der. Kuyruk kapisi bu pencereye
//! bakar; pencere disinda `Queued` isler baslatilmaz. **Calisanlar durdurulmaz**:
//! kullanicinin isini yarida kesmek surpriz olur, indirme zaten devam edebilir.
//!
//! Arayuzdeki saf hesap `src/schedule.ts` (TS) ile ayni kurallari uygular; ikisi
//! ayri dilde oldugu icin testler iki tarafta da var (`tools/test-schedule.mjs`,
//! bu dosyanin `#[cfg(test)]` blogu).

/// `"HH:MM"` → günün dakikası. Geçersizse `None`.
pub fn parse_clock(text: &str) -> Option<u32> {
    let raw = text.trim();
    if raw.is_empty() {
        return None;
    }
    let (hours, minutes) = raw.split_once(':')?;
    let hours: u32 = hours.parse().ok()?;
    let minutes: u32 = minutes.parse().ok()?;
    if hours > 23 || minutes > 59 {
        return None;
    }
    Some(hours * 60 + minutes)
}

/// Saati `"HH:MM"` biçimine getirir; geçersizse boş metin (ayar temizlenir).
pub fn normalize_clock(text: &str) -> String {
    match parse_clock(text) {
        Some(minutes) => format!("{:02}:{:02}", minutes / 60, minutes % 60),
        None => String::new(),
    }
}

/// Pencere kullanılabilir mi? İki uç da boşsa pencere yok (= her zaman açık).
pub fn window_valid(start: &str, end: &str) -> bool {
    let empty_start = start.trim().is_empty();
    let empty_end = end.trim().is_empty();
    if empty_start && empty_end {
        return true;
    }
    if empty_start || empty_end {
        return false;
    }
    match (parse_clock(start), parse_clock(end)) {
        (Some(from), Some(to)) => from != to,
        _ => false,
    }
}

/// Verilen dakika pencerenin içinde mi? Başlangıç dahil, bitiş hariç.
/// Gece yarısını aşan pencereler (23:00–05:00) desteklenir.
pub fn in_window(minute_of_day: u32, start: &str, end: &str) -> bool {
    if !window_valid(start, end) || start.trim().is_empty() {
        return true;
    }
    let (Some(from), Some(to)) = (parse_clock(start), parse_clock(end)) else {
        return true;
    };
    if from < to {
        minute_of_day >= from && minute_of_day < to
    } else {
        // Gece yarisini asan pencere: [from, 1440) ∪ [0, to)
        minute_of_day >= from || minute_of_day < to
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Settings;

    #[test]
    fn saat_ayristirilir() {
        assert_eq!(parse_clock("00:00"), Some(0));
        assert_eq!(parse_clock("02:30"), Some(150));
        assert_eq!(parse_clock("23:59"), Some(1439));
        assert_eq!(parse_clock(" 07:05 "), Some(425));
        assert_eq!(parse_clock(""), None);
        assert_eq!(parse_clock("24:00"), None);
        assert_eq!(parse_clock("12:70"), None);
        assert_eq!(parse_clock("abc"), None);
    }

    #[test]
    fn saat_normalize_edilir() {
        assert_eq!(normalize_clock("2:5"), "02:05");
        assert_eq!(normalize_clock(" 23:00 "), "23:00");
        assert_eq!(normalize_clock("bozuk"), "");
    }

    #[test]
    fn pencere_gecerliligi() {
        assert!(window_valid("", ""), "bos pencere = her zaman");
        assert!(!window_valid("02:00", ""), "tek uc gecersiz");
        assert!(!window_valid("02:00", "02:00"), "ayni saat gecersiz");
        assert!(window_valid("02:00", "06:00"));
        assert!(window_valid("23:00", "05:00"), "gece yarisini asan");
    }

    #[test]
    fn pencere_icinde_mi() {
        assert!(in_window(600, "", ""), "pencere yoksa her zaman acik");
        assert!(in_window(180, "02:00", "06:00"));
        assert!(in_window(120, "02:00", "06:00"), "baslangic dahil");
        assert!(!in_window(360, "02:00", "06:00"), "bitis haric");
        assert!(!in_window(60, "02:00", "06:00"));
        // Gece yarisini asan.
        assert!(in_window(1410, "23:00", "05:00"));
        assert!(in_window(120, "23:00", "05:00"));
        assert!(!in_window(720, "23:00", "05:00"));
        // Gecersiz pencere kuyrugu kilitlemez.
        assert!(in_window(600, "02:00", ""));
    }

    #[test]
    fn bozuk_pencere_ayarlarda_temizlenir() {
        let mut settings = Settings {
            schedule_start: "2:5".into(),
            schedule_end: "06:00".into(),
            ..Settings::default()
        };
        settings.normalize();
        assert_eq!(settings.schedule_start, "02:05", "saat normalize edildi");
        assert_eq!(settings.schedule_end, "06:00");

        settings.schedule_start = "bozuk".into();
        settings.normalize();
        assert!(
            settings.schedule_start.is_empty(),
            "bozuk baslangic temizlendi"
        );
        assert!(
            settings.schedule_end.is_empty(),
            "yarim pencere anlamsiz: ikisi de temizlenir"
        );
    }
}
