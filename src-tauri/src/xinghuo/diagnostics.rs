use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::json;
use zip::write::SimpleFileOptions;

const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
const MAX_LOG_FILES: usize = 6;

static BEARER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(Bearer\s+)[A-Za-z0-9._~+/=-]+").unwrap());
static API_KEY_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"sk-[A-Za-z0-9_-]{4,}").unwrap());
static CDK_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"XH-[A-Za-z0-9-]{8,}").unwrap());
static URL_TOKEN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)([?&](?:token|access_token|api_key|key)=)[^&\s]+").unwrap());

fn sanitize_diagnostic_text(text: &str) -> String {
    let text = BEARER_RE.replace_all(text, "$1[REDACTED]");
    let text = API_KEY_RE.replace_all(&text, "[REDACTED_KEY]");
    let text = CDK_RE.replace_all(&text, "[REDACTED_CDK]");
    URL_TOKEN_RE.replace_all(&text, "$1[REDACTED]").into_owned()
}

fn read_log_tail(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let length = file.metadata()?.len();
    if length > MAX_LOG_BYTES {
        file.seek(std::io::SeekFrom::Start(length - MAX_LOG_BYTES))?;
    }
    let mut bytes = Vec::new();
    file.take(MAX_LOG_BYTES).read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn write_text_file(
    archive: &mut zip::ZipWriter<std::fs::File>,
    name: &str,
    text: &str,
) -> Result<(), String> {
    archive
        .start_file(
            name,
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )
        .map_err(|error| error.to_string())?;
    archive
        .write_all(text.as_bytes())
        .map_err(|error| error.to_string())
}

fn log_files(log_dir: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(entries) = std::fs::read_dir(log_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("xinghuo-token"))
            {
                paths.push(path);
            }
        }
    }
    paths.sort_by_key(|path| {
        path.metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .map(std::cmp::Reverse)
    });
    paths.truncate(MAX_LOG_FILES);
    paths
}

pub(crate) fn export_diagnostics_to(output_dir: &Path, log_dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(output_dir).map_err(|error| error.to_string())?;
    let filename = format!(
        "xinghuo-debug-{}.zip",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    );
    let output_path = output_dir.join(filename);
    let file = std::fs::File::create(&output_path).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipWriter::new(file);

    let diagnostic = json!({
        "generatedAt": chrono::Local::now().to_rfc3339(),
        "appVersion": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "activation": super::current_view(),
        "credentialHandling": "CDK is exchanged for SK without device binding; SK is used as the Bearer credential",
        "privacy": "Credentials, activation records, wallets and databases are not included"
    });
    write_text_file(
        &mut archive,
        "diagnostic.json",
        &serde_json::to_string_pretty(&diagnostic).map_err(|error| error.to_string())?,
    )?;

    for path in log_files(log_dir) {
        if let Ok(text) = read_log_tail(&path) {
            let name = format!("logs/{}", path.file_name().unwrap().to_string_lossy());
            write_text_file(&mut archive, &name, &sanitize_diagnostic_text(&text))?;
        }
    }

    if let Some(config_dir) = log_dir.parent() {
        let crash_path = config_dir.join("crash.log");
        if let Ok(text) = read_log_tail(&crash_path) {
            write_text_file(
                &mut archive,
                "logs/crash.log",
                &sanitize_diagnostic_text(&text),
            )?;
        }
    }

    archive.finish().map_err(|error| error.to_string())?;
    Ok(output_path)
}

pub fn export_diagnostics() -> Result<PathBuf, String> {
    let output_dir = dirs::desktop_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Desktop")))
        .ok_or_else(|| "无法找到桌面目录".to_string())?;
    export_diagnostics_to(&output_dir, &crate::panic_hook::get_log_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn redacts_credentials_and_url_tokens() {
        let text = "Bearer sk-secret-123456 XH-ABCDE-FGHJK-MNPQR https://example.test?token=secret&x=1 XH-4001";
        let safe = sanitize_diagnostic_text(text);
        assert!(!safe.contains("sk-secret-123456"));
        assert!(!safe.contains("XH-ABCDE-FGHJK-MNPQR"));
        assert!(!safe.contains("token=secret"));
        assert!(safe.contains("XH-4001"));
        assert!(safe.contains("x=1"));
    }

    #[test]
    fn creates_sanitized_zip_without_activation_files() {
        let temp = tempfile::tempdir().unwrap();
        let log_dir = temp.path().join("logs");
        std::fs::create_dir_all(&log_dir).unwrap();
        std::fs::write(
            log_dir.join("xinghuo-token.log"),
            "request failed for sk-secret-123456 and XH-ABCDE-FGHJK-MNPQR",
        )
        .unwrap();
        let output = export_diagnostics_to(&temp.path().join("Desktop"), &log_dir).unwrap();
        let file = std::fs::File::open(output).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        assert!(archive.by_name("activation.json").is_err());
        assert!(archive.by_name("wallet.json").is_err());

        let mut log = String::new();
        archive
            .by_name("logs/xinghuo-token.log")
            .unwrap()
            .read_to_string(&mut log)
            .unwrap();
        assert!(!log.contains("sk-secret-123456"));
        assert!(!log.contains("XH-ABCDE-FGHJK-MNPQR"));

        let mut diagnostic = String::new();
        archive
            .by_name("diagnostic.json")
            .unwrap()
            .read_to_string(&mut diagnostic)
            .unwrap();
        assert!(!diagnostic.contains("\"token\""));
    }

    #[test]
    fn creates_zip_when_logs_are_missing() {
        let temp = tempfile::tempdir().unwrap();
        let output = export_diagnostics_to(
            &temp.path().join("Desktop"),
            &temp.path().join("missing-logs"),
        )
        .unwrap();
        assert!(output.exists());
    }
}
