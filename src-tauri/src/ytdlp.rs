//! yt-dlp entegrasyonu: medya sitelerinden indirme, ilerleme ve iptal.

use crate::engine::CancelToken;
use crate::manager::AppState;
use crate::model::Status;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};

const PROGRESS_PREFIX: &str = "__HYDRA__|";
const WINDOWS_NO_WINDOW: u32 = 0x0800_0000;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YtdlpStatus {
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
}

fn binary_names() -> [&'static str; 2] {
    if cfg!(windows) {
        ["yt-dlp.exe", "yt-dlp"]
    } else {
        ["yt-dlp", "yt-dlp.exe"]
    }
}

fn candidate_paths(app: &AppHandle, setting: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let setting = setting.trim();
    if !setting.is_empty() {
        out.push(PathBuf::from(setting));
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            for name in binary_names() {
                out.push(dir.join(name));
            }
        }
    }
    if let Ok(resource) = app.path().resource_dir() {
        for name in binary_names() {
            out.push(resource.join("bin").join(name));
            out.push(resource.join(name));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in binary_names() {
                out.push(dir.join("bin").join(name));
                out.push(dir.join(name));
            }
        }
    }
    if let Ok(data) = app.path().app_data_dir() {
        for name in binary_names() {
            out.push(data.join("bin").join(name));
        }
    }
    out
}

/// yt-dlp'nin resmi son sürüm ikilisi (Windows).
pub const YTDLP_DOWNLOAD_URL: &str =
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    /// "downloading" | "verifying" | "done" | "failed"
    pub stage: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Kurulumun yazacagi yol: `<app_data>/bin/yt-dlp.exe`.
pub fn install_target(app: &AppHandle) -> Result<PathBuf, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Uygulama klasörü bulunamadı: {e}"))?;
    Ok(data.join("bin").join(binary_names()[0]))
}

/// yt-dlp'yi indirip `<app_data>/bin` altina kurar, sonra `--version` ile dogrular.
/// `on_progress` her asamada cagrilir (ag katmani olmadan test edilebilir).
pub async fn install(
    app: &AppHandle,
    client: &reqwest::Client,
    on_progress: impl Fn(InstallProgress) + Send + Sync,
) -> Result<PathBuf, String> {
    use futures_util::StreamExt;
    use std::io::Write;

    let target = install_target(app)?;
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Klasör oluşturulamadı: {e}"))?;
    }
    let tmp = target.with_extension("exe.part");

    let response = client
        .get(YTDLP_DOWNLOAD_URL)
        .header(reqwest::header::ACCEPT, "application/octet-stream")
        .send()
        .await
        .map_err(|e| format!("İndirme başlatılamadı: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "yt-dlp indirilemedi (HTTP {}). İnternet bağlantısını kontrol edin.",
            response.status()
        ));
    }
    let total = response.content_length();
    let mut downloaded: u64 = 0;
    on_progress(InstallProgress {
        stage: "downloading".into(),
        downloaded: 0,
        total,
        error: None,
    });

    let mut file = std::fs::File::create(&tmp).map_err(|e| format!("Dosya açılamadı: {e}"))?;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("İndirme kesildi: {e}")
        })?;
        file.write_all(&chunk)
            .map_err(|e| format!("Diske yazılamadı: {e}"))?;
        downloaded += chunk.len() as u64;
        on_progress(InstallProgress {
            stage: "downloading".into(),
            downloaded,
            total,
            error: None,
        });
    }
    file.flush().map_err(|e| format!("Diske yazılamadı: {e}"))?;
    drop(file);

    // Küçük dosya = hata sayfası indirilmiş olabilir (GitHub bazen HTML döner).
    if downloaded < 1024 * 1024 {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "İndirilen dosya çok küçük ({downloaded} bayt); kurulum iptal edildi."
        ));
    }

    on_progress(InstallProgress {
        stage: "verifying".into(),
        downloaded,
        total,
        error: None,
    });
    std::fs::rename(&tmp, &target).map_err(|e| format!("Yerine konulamadı: {e}"))?;

    // Çalışıyor mu? Bozuksa geri al.
    let mut cmd = tokio::process::Command::new(&target);
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        cmd.creation_flags(WINDOWS_NO_WINDOW);
    }
    let check = tokio::time::timeout(Duration::from_secs(20), cmd.output()).await;
    let ok = matches!(&check, Ok(Ok(output)) if output.status.success());
    if !ok {
        let _ = std::fs::remove_file(&target);
        return Err(
            "yt-dlp indirildi ama çalıştırılamadı (virüs koruması engellemiş olabilir).".into(),
        );
    }

    on_progress(InstallProgress {
        stage: "done".into(),
        downloaded,
        total,
        error: None,
    });
    Ok(target)
}

/// Ayarlardaki yol, PATH ve uygulama klasorlerinde yt-dlp arar.
pub fn resolve(app: &AppHandle, setting: &str) -> Option<PathBuf> {
    candidate_paths(app, setting)
        .into_iter()
        .find(|path| path.is_file())
}

pub async fn status(app: &AppHandle, setting: &str) -> YtdlpStatus {
    let Some(path) = resolve(app, setting) else {
        return YtdlpStatus {
            available: false,
            path: None,
            version: None,
        };
    };
    let mut cmd = tokio::process::Command::new(&path);
    cmd.arg("--version");
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::null());
    #[cfg(windows)]
    {
        cmd.creation_flags(WINDOWS_NO_WINDOW);
    }
    let version = match tokio::time::timeout(Duration::from_secs(8), cmd.output()).await {
        Ok(Ok(output)) if output.status.success() => {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        }
        _ => None,
    };
    YtdlpStatus {
        available: true,
        path: Some(path.to_string_lossy().to_string()),
        version,
    }
}

/// yt-dlp'nin `-J` ciktisindaki tek format.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct YtdlpFormat {
    pub format_id: String,
    #[serde(default)]
    pub ext: Option<String>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub filesize: Option<u64>,
    #[serde(default)]
    pub filesize_approx: Option<u64>,
    #[serde(default)]
    pub vcodec: Option<String>,
    #[serde(default)]
    pub acodec: Option<String>,
    #[serde(default)]
    pub format_note: Option<String>,
    #[serde(default)]
    pub tbr: Option<f64>,
    #[serde(default)]
    pub protocol: Option<String>,
}

impl YtdlpFormat {
    pub fn label(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        match self.height {
            Some(h) if h > 0 => parts.push(format!("{h}p")),
            _ => {
                if let Some(note) = &self.format_note {
                    if !note.is_empty() {
                        parts.push(note.clone());
                    }
                }
            }
        }
        if let Some(ext) = &self.ext {
            parts.push(ext.clone());
        }
        if let Some(size) = self.filesize.or(self.filesize_approx) {
            parts.push(format!("{:.1} MB", size as f64 / (1024.0 * 1024.0)));
        }
        let only_audio = self.vcodec.as_deref() == Some("none");
        if only_audio && self.acodec.as_deref() != Some("none") {
            parts.push("ses".into());
        }
        if parts.is_empty() {
            self.format_id.clone()
        } else {
            parts.join(" · ")
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct YtdlpInfo {
    pub title: Option<String>,
    pub formats: Vec<YtdlpFormat>,
    pub is_live: bool,
    pub duration_secs: Option<f64>,
}

pub fn parse_info(json: &str) -> Result<YtdlpInfo, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("yt-dlp JSON çözülemedi: {e}"))?;
    let title = value
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let is_live = value
        .get("is_live")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let duration_secs = value.get("duration").and_then(|v| v.as_f64());
    let mut formats = Vec::new();
    if let Some(list) = value.get("formats").and_then(|v| v.as_array()) {
        for entry in list {
            // `-J` bazi alanlari null dondurur; deserialize bunu Option ile tolere eder.
            match serde_json::from_value::<YtdlpFormat>(entry.clone()) {
                Ok(format) => formats.push(format),
                Err(_) => continue,
            }
        }
    }
    Ok(YtdlpInfo {
        title,
        formats,
        is_live,
        duration_secs,
    })
}

/// `yt-dlp -J` ile sayfadaki formatlari listeler.
pub async fn list_formats(binary: &std::path::Path, url: &str) -> Result<YtdlpInfo, String> {
    let mut cmd = tokio::process::Command::new(binary);
    cmd.args([
        "-J",
        "--no-warnings",
        "--no-playlist",
        "--skip-download",
        "--no-check-certificates",
        url,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        cmd.creation_flags(WINDOWS_NO_WINDOW);
    }
    let output = cmd
        .output()
        .await
        .map_err(|e| format!("yt-dlp çalıştırılamadı: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let last = stderr
            .lines()
            .rfind(|l| !l.trim().is_empty())
            .unwrap_or("bilinmeyen hata");
        return Err(format!("yt-dlp: {last}"));
    }
    parse_info(&String::from_utf8_lossy(&output.stdout))
}

fn parse_u64_loose(raw: &str) -> Option<u64> {
    let raw = raw.trim().replace(',', "");
    if raw.is_empty() || raw.eq_ignore_ascii_case("na") || raw == "None" {
        return None;
    }
    raw.parse::<u64>()
        .ok()
        .or_else(|| raw.parse::<f64>().ok().map(|v| v.max(0.0) as u64))
}

/// yt-dlp ilerleme satirini cozer: `indirilen|tahmini|kesin|yuzde`.
fn parse_progress(payload: &str) -> Option<(u64, Option<u64>, Option<f64>)> {
    let mut parts = payload.split('|');
    let downloaded = parse_u64_loose(parts.next()?)?;
    let estimate = parse_u64_loose(parts.next().unwrap_or(""));
    let exact = parse_u64_loose(parts.next().unwrap_or(""));
    let percent_raw = parts.next().unwrap_or("").trim().trim_end_matches('%');
    let percent = percent_raw.parse::<f64>().ok();
    Some((downloaded, exact.or(estimate), percent))
}

/// yt-dlp'nin kendi dosya adini kurmasini engelliyoruz: basliklar Windows'ta
/// yasak karakter (`:`, `?`, `|`) icerebiliyor ve `--windows-filenames` bile
/// hepsini temizlemiyor (NTFS'te "alternate data stream" olusup errno 22
/// veriyordu). Bu yuzden yt-dlp yalnizca guvenli `%(id)s.%(ext)s` yazar;
/// gorunen adi biz `util::sanitize_filename` ile kurariz.
const SAFE_OUTPUT_TEMPLATE: &str = "%(id)s.%(ext)s";

/// Gorunen dosya adi: temizlenmis baslik + uzanti. Baslik bossa `None`.
pub fn display_filename(title: &str, ext: &str) -> Option<String> {
    let title = title.trim();
    if title.is_empty() {
        return None;
    }
    let base = crate::util::sanitize_filename(title);
    if base == "download" {
        return None;
    }
    let ext = ext.trim().trim_start_matches('.');
    Some(if ext.is_empty() {
        base
    } else {
        format!("{base}.{ext}")
    })
}

fn build_args(
    url: &str,
    dir: &Path,
    print_file: &Path,
    title_file: &Path,
    format: Option<&str>,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--newline".into(),
        "--no-playlist".into(),
        "--no-mtime".into(),
        "--continue".into(),
        // Basliktaki yasak karakterler dosya adina sizmasin.
        "--windows-filenames".into(),
        "--progress".into(),
        "--progress-template".into(),
        format!(
            "download:{PROGRESS_PREFIX}%(progress.downloaded_bytes)s|%(progress.total_bytes_estimate)s|%(progress.total_bytes)s|%(progress._percent_str)s"
        ),
        "--print-to-file".into(),
        "after_move:filepath".into(),
        print_file.to_string_lossy().to_string(),
        "--print-to-file".into(),
        "before_dl:%(title)s".into(),
        title_file.to_string_lossy().to_string(),
    ];
    // Format URL ise (HLS varyanti) dogrudan onu indir.
    if let Some(format) = format.filter(|f| !f.is_empty()) {
        if format.starts_with("http://") || format.starts_with("https://") {
            args.push("--no-check-certificates".into());
            args.push(format.to_string());
            args.push("-P".into());
            args.push(dir.to_string_lossy().to_string());
            args.push("-o".into());
            args.push(SAFE_OUTPUT_TEMPLATE.into());
            return args;
        }
        args.push("-f".into());
        args.push(format.to_string());
        // Ses+video ayriysa tek dosyada birlestir.
        args.push("--merge-output-format".into());
        args.push("mp4".into());
    }
    args.push("-P".into());
    args.push(dir.to_string_lossy().to_string());
    args.push("-o".into());
    args.push(SAFE_OUTPUT_TEMPLATE.into());
    args.push(url.into());
    args
}

pub async fn run(state: Arc<AppState>, id: String, cancel: Arc<CancelToken>) {
    let Some(item) = state.item(&id) else { return };
    if item.status != Status::Downloading {
        return;
    }
    let settings = state.settings();
    let Some(binary) = resolve(&state.app, &settings.ytdlp_path) else {
        state.update_item(&id, |d| {
            d.status = Status::Failed;
            d.error = Some("yt-dlp bulunamadi".into());
        });
        state.persist();
        state.emit_downloads();
        return;
    };

    let parts = state.parts_dir(&id);
    let _ = tokio::fs::create_dir_all(&parts).await;
    let print_file = parts.join("filepath.txt");
    let _ = tokio::fs::remove_file(&print_file).await;
    let title_file = parts.join("title.txt");
    let _ = tokio::fs::remove_file(&title_file).await;

    let mut cmd = tokio::process::Command::new(binary);
    cmd.args(build_args(
        &item.url,
        &item.dir,
        &print_file,
        &title_file,
        item.format.as_deref(),
    ))
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        cmd.creation_flags(WINDOWS_NO_WINDOW);
    }

    let started_at = std::time::SystemTime::now();
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            state.update_item(&id, |d| {
                d.status = Status::Failed;
                d.error = Some(format!("yt-dlp baslatilamadi: {err}"));
            });
            state.persist();
            state.emit_downloads();
            return;
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let reader_state = state.clone();
    let reader_id = id.clone();
    let reader = stdout.map(|stdout| {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Some(payload) = line.strip_prefix(PROGRESS_PREFIX) else {
                    continue;
                };
                if let Some((downloaded, total, percent)) = parse_progress(payload) {
                    reader_state.update_item(&reader_id, |d| {
                        d.downloaded_bytes = downloaded;
                        if total.is_some() {
                            d.total_bytes = total;
                        }
                        d.progress = percent
                            .map(|p| (p / 100.0).clamp(0.0, 1.0))
                            .or_else(|| {
                                total.map(|t| (downloaded as f64 / t as f64).clamp(0.0, 1.0))
                            })
                            .or(d.progress);
                    });
                }
            }
        })
    });

    let stderr_task = stderr.map(|stderr| {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            let mut tail: Vec<String> = Vec::new();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    tail.push(line);
                    if tail.len() > 40 {
                        tail.remove(0);
                    }
                }
            }
            tail.join("\n")
        })
    });

    let status = tokio::select! {
        _ = cancel.cancelled() => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            None
        }
        result = child.wait() => Some(result),
    };

    if let Some(reader) = reader {
        let _ = tokio::time::timeout(Duration::from_secs(2), reader).await;
    }
    let stderr_tail = match stderr_task {
        Some(task) => tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or_default(),
        None => String::new(),
    };

    if cancel.is_cancelled() {
        // Durumu komut (duraklat/iptal) belirledi; burada yalnizca hizi sifirla.
        state.update_item(&id, |d| d.speed_bps = 0);
        state.persist();
        state.emit_downloads();
        return;
    }

    let success = matches!(&status, Some(Ok(s)) if s.success());
    if !success {
        let detail = if stderr_tail.trim().is_empty() {
            status
                .and_then(|r| r.ok())
                .map(|s| format!("yt-dlp {s} ile cikti"))
                .unwrap_or_else(|| "yt-dlp calistirilamadi".into())
        } else {
            stderr_tail
                .lines()
                .last()
                .unwrap_or("yt-dlp hatasi")
                .to_string()
        };
        state.update_item(&id, |d| {
            d.status = Status::Failed;
            d.speed_bps = 0;
            d.error = Some(detail);
            d.finished_at = Some(chrono::Utc::now().timestamp());
        });
        state.persist();
        state.emit_downloads();
        return;
    }

    let printed = tokio::fs::read_to_string(&print_file)
        .await
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let path = printed
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .or_else(|| newest_file_in(&item.dir, started_at));

    let Some(path) = path else {
        state.update_item(&id, |d| {
            d.status = Status::Failed;
            d.error = Some("yt-dlp bitti ama olusan dosya bulunamadi".into());
        });
        state.persist();
        state.emit_downloads();
        return;
    };

    // yt-dlp guvenli `%(id)s.%(ext)s` ile indirdi; gorunen adi burada kuruyoruz.
    let title = tokio::fs::read_to_string(&title_file)
        .await
        .ok()
        .and_then(|raw| {
            raw.lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .map(str::to_string)
        });
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_string();
    let path = rename_to_display_name(&path, title.as_deref(), &ext);

    let size = tokio::fs::metadata(&path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&item.filename)
        .to_string();
    let _ = tokio::fs::remove_dir_all(&parts).await;

    state.update_item(&id, |d| {
        d.status = Status::Completed;
        d.filename = filename.clone();
        d.total_bytes = Some(size);
        d.downloaded_bytes = size;
        d.progress = Some(1.0);
        d.speed_bps = 0;
        d.error = None;
        d.finished_at = Some(chrono::Utc::now().timestamp());
        d.file_path = Some(path.clone());
    });
    state.persist();
    state.emit_downloads();
    state.notice(format!("Tamamlandi: {filename}"), "success");
}

/// Indirilen dosyayi baslik tabanli, Windows-guvenli bir ada tasir.
/// Yeniden adlandirma basarisiz olursa eski yol korunur (indirme bozulmaz).
fn rename_to_display_name(path: &Path, title: Option<&str>, ext: &str) -> PathBuf {
    let Some(title) = title else {
        return path.to_path_buf();
    };
    let Some(name) = display_filename(title, ext) else {
        return path.to_path_buf();
    };
    let dir = match path.parent() {
        Some(dir) => dir,
        None => return path.to_path_buf(),
    };
    let target = crate::util::unique_path(dir, &name);
    if target == path {
        return path.to_path_buf();
    }
    match std::fs::rename(path, &target) {
        Ok(()) => target,
        Err(err) => {
            eprintln!("[hydra] yeniden adlandirma basarisiz: {err}");
            path.to_path_buf()
        }
    }
}

fn newest_file_in(dir: &Path, since: std::time::SystemTime) -> Option<PathBuf> {
    let threshold = since.checked_sub(Duration::from_secs(5)).unwrap_or(since);
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Ok(modified) = entry.metadata().and_then(|m| m.modified()) {
            if modified < threshold {
                continue;
            }
            if best.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) {
                best = Some((modified, path));
            }
        }
    }
    best.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ilerleme_satiri_cozulur() {
        assert_eq!(
            parse_progress("1024|2048|4096| 25.0%"),
            Some((1024, Some(4096), Some(25.0)))
        );
        assert_eq!(parse_progress("NA|NA|NA|NA"), None);
        assert_eq!(
            parse_progress("500|1000|NA|50.0%"),
            Some((500, Some(1000), Some(50.0)))
        );
        assert_eq!(parse_progress("1,234|NA|NA|NA"), Some((1234, None, None)));
    }

    #[test]
    fn argumanlar_guvenli() {
        let args = build_args(
            "https://x/y",
            Path::new("C:/out"),
            Path::new("C:/tmp/p.txt"),
            Path::new("C:/tmp/t.txt"),
            None,
        );
        assert!(args.contains(&"--no-playlist".to_string()));
        assert!(args.contains(&"--continue".to_string()));
        assert!(args.contains(&"C:/out".to_string()));
        assert_eq!(args.last().unwrap(), "https://x/y");
        assert!(args.iter().any(|a| a.contains(PROGRESS_PREFIX)));
    }

    #[test]
    fn format_verilirse_f_eklenir() {
        let args = build_args(
            "https://x/y",
            Path::new("C:/out"),
            Path::new("C:/tmp/p.txt"),
            Path::new("C:/tmp/t.txt"),
            Some("137+140"),
        );
        let index = args.iter().position(|a| a == "-f").expect("-f yok");
        assert_eq!(args[index + 1], "137+140");
        assert_eq!(args.last().unwrap(), "https://x/y");
    }

    #[test]
    fn manifest_formati_url_olarak_indirilir() {
        let args = build_args(
            "https://x/y",
            Path::new("C:/out"),
            Path::new("C:/tmp/p.txt"),
            Path::new("C:/tmp/t.txt"),
            Some("https://cdn/x/1080.m3u8"),
        );
        assert!(args.contains(&"https://cdn/x/1080.m3u8".to_string()));
        assert!(!args.contains(&"-f".to_string()));
        assert_eq!(args.last().unwrap(), "%(id)s.%(ext)s");
    }

    #[test]
    fn format_listesi_cozulur() {
        let json = serde_json::json!({
            "title": "Ornek Video",
            "is_live": false,
            "formats": [
                {"format_id": "137", "ext": "mp4", "height": 1080, "filesize": 50_000_000,
                 "vcodec": "avc1.640028", "acodec": "none", "format_note": "1080p"},
                {"format_id": "140", "ext": "m4a", "height": null, "filesize_approx": 4_000_000,
                 "vcodec": "none", "acodec": "mp4a.40.2", "format_note": "medium"},
                {"format_id": "sb0", "ext": "mhtml", "vcodec": "none", "acodec": "none"}
            ]
        });
        let info = parse_info(&json.to_string()).expect("json cozulemedi");
        assert_eq!(info.title.as_deref(), Some("Ornek Video"));
        assert_eq!(info.formats.len(), 3, "parse_info ham listeyi dondurur");
        assert_eq!(info.formats[0].height, Some(1080));
        assert!(info.formats[0].label().contains("1080p"));
        let audio = info
            .formats
            .iter()
            .find(|f| f.format_id == "140")
            .expect("ses formati yok");
        assert_eq!(audio.filesize.or(audio.filesize_approx), Some(4_000_000));
        assert!(audio.label().contains("ses"));
    }

    #[test]
    fn gorunen_ad_windows_guvenli() {
        // Gercek hata: basliktaki ':' Windows'ta yasak, yt-dlp errno 22 veriyordu.
        let name = display_filename(
            "Kimi K3.1 Pro'yu Canlı Zorluyoruz: Açık Model Nereye Kadar Gider?",
            "mp4",
        )
        .expect("ad uretilmedi");
        assert!(!name.contains(':'), "iki nokta kalmamali: {name}");
        assert!(name.ends_with(".mp4"));
        assert!(name.contains("Kimi K3.1"));

        // Baslik bossa ad uretme, mevcut ad korunur.
        assert_eq!(display_filename("   ", "mp4"), None);
        // Uzanti noktayla da verilse tek nokta kalsin.
        assert_eq!(
            display_filename("rapor", ".pdf").as_deref(),
            Some("rapor.pdf")
        );
        // Uzanti bossa nokta eklenmesin.
        assert_eq!(display_filename("arsiv", "").as_deref(), Some("arsiv"));
    }

    #[test]
    fn yt_dlp_guvenli_sablon_kullanir() {
        let args = build_args(
            "https://x/y",
            Path::new("C:/out"),
            Path::new("C:/tmp/p.txt"),
            Path::new("C:/tmp/t.txt"),
            Some("137"),
        );
        let idx = args.iter().position(|a| a == "-o").expect("-o yok");
        assert_eq!(args[idx + 1], "%(id)s.%(ext)s", "baslik sablonda olmamali");
        assert!(args.contains(&"--windows-filenames".to_string()));
        // Baslik ayri dosyaya yazilmali (gorunen ad icin).
        let pidx = args
            .iter()
            .position(|a| a == "before_dl:%(title)s")
            .expect("baslik yazdirma yok");
        assert_eq!(args[pidx + 1], "C:/tmp/t.txt");
    }

    #[test]
    fn yeniden_adlandirma_guvenli_ada_tasir() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("dQw4w9WgXcQ.mp4");
        std::fs::write(&src, b"x").unwrap();
        let out = rename_to_display_name(
            &src,
            Some("Kimi K3.1 Pro'yu Canlı Zorluyoruz: Açık Model"),
            "mp4",
        );
        assert!(out.is_file(), "hedef dosya yok: {out:?}");
        assert!(!src.exists(), "eski dosya kalmamali");
        let name = out.file_name().unwrap().to_string_lossy().to_string();
        assert!(!name.contains(':'), "yasak karakter: {name}");
        assert!(name.ends_with(".mp4"));

        // Baslik yoksa dosya yerinde kalir.
        let out2 = rename_to_display_name(&out, None, "mp4");
        assert_eq!(out2, out);
    }

    #[test]
    fn kurulum_sabitleri_tutarli() {
        assert!(YTDLP_DOWNLOAD_URL.starts_with("https://github.com/yt-dlp/yt-dlp/releases/"));
        assert!(YTDLP_DOWNLOAD_URL.ends_with("yt-dlp.exe"));
        assert!(binary_names()[0].starts_with("yt-dlp"));
    }
}
