use std::time::Duration;

use serde::{Deserialize, Serialize};

#[cfg(test)]
const GITHUB_REPOSITORY: &str = "Felix198902/xinghuo-token-releases";
#[cfg(test)]
const GITEE_REPOSITORY: &str = "jerrylinap/xinghuo-token-releases";
const GITHUB_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/Felix198902/xinghuo-token-releases/releases/latest";
const GITEE_LATEST_RELEASE_URL: &str =
    "https://gitee.com/api/v5/repos/jerrylinap/xinghuo-token-releases/releases/latest";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy)]
struct ReleaseSource {
    name: &'static str,
    releases_url: &'static str,
    allowed_url_prefix: &'static str,
}

const GITHUB_SOURCE: ReleaseSource = ReleaseSource {
    name: "GitHub",
    releases_url: "https://github.com/Felix198902/xinghuo-token-releases/releases",
    allowed_url_prefix: "https://github.com/",
};
const GITEE_SOURCE: ReleaseSource = ReleaseSource {
    name: "Gitee",
    releases_url: "https://gitee.com/jerrylinap/xinghuo-token-releases/releases",
    allowed_url_prefix: "https://gitee.com/",
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateInfo {
    pub available: bool,
    pub current_version: String,
    pub latest_version: String,
    pub title: Option<String>,
    pub notes: Option<String>,
    pub download_url: String,
    pub source: String,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
}

fn normalize_version(version: &str) -> &str {
    version
        .trim()
        .strip_prefix(['v', 'V'])
        .unwrap_or(version.trim())
}

fn compare_versions(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    fn parse(version: &str) -> Option<([u64; 3], Vec<&str>)> {
        let version = normalize_version(version);
        let (core, pre) = version.split_once('-').unwrap_or((version, ""));
        let mut parts = core.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some((
            [major, minor, patch],
            if pre.is_empty() {
                Vec::new()
            } else {
                pre.split('.').collect()
            },
        ))
    }

    let (left_core, left_pre) = parse(left)?;
    let (right_core, right_pre) = parse(right)?;
    let core_order = left_core.cmp(&right_core);
    if core_order != std::cmp::Ordering::Equal {
        return Some(core_order);
    }
    match (left_pre.is_empty(), right_pre.is_empty()) {
        (true, true) => Some(std::cmp::Ordering::Equal),
        (true, false) => Some(std::cmp::Ordering::Greater),
        (false, true) => Some(std::cmp::Ordering::Less),
        (false, false) => {
            for (left, right) in left_pre.iter().zip(&right_pre) {
                let order = match (left.parse::<u64>(), right.parse::<u64>()) {
                    (Ok(left), Ok(right)) => left.cmp(&right),
                    (Ok(_), Err(_)) => std::cmp::Ordering::Less,
                    (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
                    (Err(_), Err(_)) => left.cmp(right),
                };
                if order != std::cmp::Ordering::Equal {
                    return Some(order);
                }
            }
            Some(left_pre.len().cmp(&right_pre.len()))
        }
    }
}

fn parse_release(
    current_version: &str,
    source: ReleaseSource,
    body: &str,
) -> Result<AppUpdateInfo, String> {
    let release: Release = serde_json::from_str(body)
        .map_err(|error| format!("{} 响应格式无效: {error}", source.name))?;
    let latest_version = normalize_version(&release.tag_name).to_string();
    if latest_version.is_empty() {
        return Err(format!("{} Release 缺少版本号", source.name));
    }
    let available = compare_versions(&latest_version, current_version)
        .is_some_and(|order| order == std::cmp::Ordering::Greater);
    let download_url = release
        .html_url
        .filter(|url| url.starts_with(source.allowed_url_prefix))
        .unwrap_or_else(|| source.releases_url.to_string());
    Ok(AppUpdateInfo {
        available,
        current_version: normalize_version(current_version).to_string(),
        latest_version,
        title: release.name.filter(|name| !name.trim().is_empty()),
        notes: release.body.filter(|notes| !notes.trim().is_empty()),
        download_url,
        source: source.name.to_string(),
    })
}

async fn fetch_release(
    current_version: &str,
    source: ReleaseSource,
    api_url: &str,
) -> Result<AppUpdateInfo, String> {
    let response = crate::proxy::http_client::get()
        .get(api_url)
        .timeout(REQUEST_TIMEOUT)
        .header(
            "User-Agent",
            concat!("xinghuo-token/", env!("CARGO_PKG_VERSION")),
        )
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| format!("无法连接 {}: {error}", source.name))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取 {} 响应失败: {error}", source.name))?;
    if !status.is_success() {
        return Err(format!("{} 检查更新失败 (HTTP {status})", source.name));
    }
    parse_release(current_version, source, &body)
}

fn select_newer_release(github: AppUpdateInfo, gitee: AppUpdateInfo) -> AppUpdateInfo {
    if compare_versions(&gitee.latest_version, &github.latest_version)
        .is_some_and(|order| order == std::cmp::Ordering::Greater)
    {
        gitee
    } else {
        github
    }
}

async fn check_update_from(
    current_version: &str,
    github_url: &str,
    gitee_url: &str,
) -> Result<AppUpdateInfo, String> {
    let (github, gitee) = tokio::join!(
        fetch_release(current_version, GITHUB_SOURCE, github_url),
        fetch_release(current_version, GITEE_SOURCE, gitee_url),
    );
    match (github, gitee) {
        (Ok(github), Ok(gitee)) => Ok(select_newer_release(github, gitee)),
        (Ok(update), Err(error)) | (Err(error), Ok(update)) => {
            log::warn!("[update] 一个更新源不可用，使用备用来源: {error}");
            Ok(update)
        }
        (Err(github), Err(gitee)) => Err(format!("{github}；{gitee}")),
    }
}

#[tauri::command]
pub async fn check_app_update() -> Result<AppUpdateInfo, String> {
    check_update_from(
        env!("CARGO_PKG_VERSION"),
        GITHUB_LATEST_RELEASE_URL,
        GITEE_LATEST_RELEASE_URL,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::StatusCode, routing::get, Router};

    fn release(tag: &str, url: &str) -> String {
        serde_json::json!({
            "tag_name": tag,
            "name": "星火更新",
            "body": "修复激活并加入双通道更新",
            "html_url": url,
        })
        .to_string()
    }

    async fn serve_releases(
        github: (StatusCode, String),
        gitee: (StatusCode, String),
    ) -> (String, tokio::task::JoinHandle<()>) {
        let github_route = move || {
            let (status, body) = github.clone();
            async move { (status, body) }
        };
        let gitee_route = move || {
            let (status, body) = gitee.clone();
            async move { (status, body) }
        };
        let app = Router::new()
            .route("/github", get(github_route))
            .route("/gitee", get(gitee_route));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{address}"), task)
    }

    #[test]
    fn newer_release_is_available_and_v_prefix_is_normalized() {
        let update = parse_release(
            "3.20.4",
            GITEE_SOURCE,
            &release(
                "v3.21.0",
                "https://gitee.com/wfpromax/xinghuo-token/releases/v3.21.0",
            ),
        )
        .unwrap();
        assert!(update.available);
        assert_eq!(update.latest_version, "3.21.0");
        assert_eq!(update.current_version, "3.20.4");
        assert_eq!(update.source, "Gitee");
        assert_eq!(update.title.as_deref(), Some("星火更新"));
    }

    #[test]
    fn same_or_older_release_does_not_prompt() {
        for tag in ["v3.20.4", "3.20.3", "3.20.4-beta.2"] {
            let body = release(tag, "https://github.com/wfpromax/xinghuo-token/releases");
            assert!(
                !parse_release("3.20.4", GITHUB_SOURCE, &body)
                    .unwrap()
                    .available
            );
        }
        assert!(
            parse_release(
                "3.20.4",
                GITHUB_SOURCE,
                &release(
                    "3.20.5",
                    "https://github.com/wfpromax/xinghuo-token/releases"
                ),
            )
            .unwrap()
            .available
        );
    }

    #[test]
    fn invalid_or_non_semver_release_fails_closed() {
        assert!(
            !parse_release(
                "3.20.4",
                GITEE_SOURCE,
                &release(
                    "nightly",
                    "https://gitee.com/wfpromax/xinghuo-token/releases"
                ),
            )
            .unwrap()
            .available
        );
        assert!(parse_release("3.20.4", GITEE_SOURCE, "not json").is_err());
        assert!(parse_release("3.20.4", GITEE_SOURCE, r#"{"tag_name":"  "}"#).is_err());
    }

    #[test]
    fn download_url_is_limited_to_the_source_https_host() {
        for source in [GITHUB_SOURCE, GITEE_SOURCE] {
            let update = parse_release(
                "3.20.4",
                source,
                &release("3.21.0", "https://example.com/malicious"),
            )
            .unwrap();
            assert_eq!(update.download_url, source.releases_url);
        }
        assert!(GITHUB_LATEST_RELEASE_URL.contains(GITHUB_REPOSITORY));
        assert!(GITEE_LATEST_RELEASE_URL.contains(GITEE_REPOSITORY));
    }

    #[tokio::test]
    async fn github_is_used_when_gitee_fails() {
        let (base, server) = serve_releases(
            (
                StatusCode::OK,
                release(
                    "v3.21.0",
                    "https://github.com/wfpromax/xinghuo-token/releases/v3.21.0",
                ),
            ),
            (StatusCode::SERVICE_UNAVAILABLE, String::new()),
        )
        .await;
        let update = check_update_from(
            "3.20.4",
            &format!("{base}/github"),
            &format!("{base}/gitee"),
        )
        .await
        .unwrap();
        assert_eq!(update.source, "GitHub");
        assert_eq!(update.latest_version, "3.21.0");
        server.abort();
    }

    #[tokio::test]
    async fn gitee_is_used_when_github_fails() {
        let (base, server) = serve_releases(
            (StatusCode::NOT_FOUND, String::new()),
            (
                StatusCode::OK,
                release(
                    "v3.21.0",
                    "https://gitee.com/wfpromax/xinghuo-token/releases/v3.21.0",
                ),
            ),
        )
        .await;
        let update = check_update_from(
            "3.20.4",
            &format!("{base}/github"),
            &format!("{base}/gitee"),
        )
        .await
        .unwrap();
        assert_eq!(update.source, "Gitee");
        server.abort();
    }

    #[tokio::test]
    async fn both_sources_are_compared_and_fail_together() {
        let (base, server) = serve_releases(
            (
                StatusCode::OK,
                release(
                    "3.21.0",
                    "https://github.com/wfpromax/xinghuo-token/releases/v3.21.0",
                ),
            ),
            (
                StatusCode::OK,
                release(
                    "3.22.0",
                    "https://gitee.com/wfpromax/xinghuo-token/releases/v3.22.0",
                ),
            ),
        )
        .await;
        let update = check_update_from(
            "3.20.4",
            &format!("{base}/github"),
            &format!("{base}/gitee"),
        )
        .await
        .unwrap();
        assert_eq!(update.latest_version, "3.22.0");
        assert_eq!(update.source, "Gitee");
        server.abort();

        let (base, server) = serve_releases(
            (StatusCode::NOT_FOUND, String::new()),
            (StatusCode::SERVICE_UNAVAILABLE, String::new()),
        )
        .await;
        let error = check_update_from(
            "3.20.4",
            &format!("{base}/github"),
            &format!("{base}/gitee"),
        )
        .await
        .unwrap_err();
        assert!(error.contains("GitHub") && error.contains("Gitee"));
        server.abort();
    }
}
