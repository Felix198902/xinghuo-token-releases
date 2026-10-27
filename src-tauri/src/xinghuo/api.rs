//! 星火服务端接口：凭据状态、客户端配置、模型列表。
//!
//! 插件契约的错误形状是 `{ok:false, code, message}`。5xx 或不是业务回包
//! （空响应、HTML 错误页）一律折成 `HTTP_ERROR`，顾客只看到 `XH-4<状态码>`。

use std::collections::BTreeMap;
use std::time::Duration;

use reqwest::{Client, RequestBuilder};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{ManagedApp, Quota, XinghuoError};

const ACTIVATE_PATH: &str = "/api/v1/plugin/activate";
const STATUS_PATH: &str = "/api/v1/plugin/status";
const CLIENT_CONFIG_PATH: &str = "/api/v1/plugin/client-config";
const MODELS_PATH: &str = "/v1/models";

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
/// 瞬断重试的等待节奏（与 xinghuo-connect 面板一致的前两档）。
const RETRY_WAITS_MS: [u64; 2] = [300, 800];

const CLIENT_ID: &str = concat!("xinghuo-token/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone)]
pub(crate) struct HttpReply {
    pub status: u16,
    pub body: String,
}

fn network_error(error: &reqwest::Error) -> XinghuoError {
    let kind = if error.is_timeout() {
        "timeout"
    } else {
        "unreachable"
    };
    XinghuoError::new("NETWORK", kind)
}

/// 发请求并在瞬断时重试。
///
/// `idempotent = false` 只在连接都没建起来时重试；查询类请求在超时时也会重试。
/// 已收到响应的 429 不重试，避免进一步触发限流。
async fn send(
    build: impl Fn(&Client) -> RequestBuilder,
    idempotent: bool,
    operation: &'static str,
) -> Result<HttpReply, XinghuoError> {
    let client = crate::proxy::http_client::get();
    let mut attempt = 0;
    loop {
        let result = build(&client)
            .timeout(REQUEST_TIMEOUT)
            .header("User-Agent", CLIENT_ID)
            .send()
            .await;
        match result {
            Ok(response) => {
                let status = response.status().as_u16();
                if status >= 400 {
                    log::warn!("[xinghuo] {operation} returned HTTP {status}");
                }
                let body = response.text().await.map_err(|e| network_error(&e))?;
                return Ok(HttpReply { status, body });
            }
            Err(error) => {
                let kind = if error.is_timeout() {
                    "timeout"
                } else {
                    "connect"
                };
                log::warn!(
                    "[xinghuo] {operation} failed: {kind}, attempt {}",
                    attempt + 1
                );
                let retryable = error.is_connect() || (idempotent && error.is_timeout());
                if retryable && attempt < RETRY_WAITS_MS.len() {
                    tokio::time::sleep(Duration::from_millis(RETRY_WAITS_MS[attempt])).await;
                    attempt += 1;
                    continue;
                }
                return Err(network_error(&error));
            }
        }
    }
}

/// 把非 200 回包转成错误：4xx 的业务码原样透传，其余只留状态码。
pub(crate) fn reply_error(reply: &HttpReply) -> XinghuoError {
    if reply.status < 500 {
        if let Ok(body) = serde_json::from_str::<Value>(&reply.body) {
            if let Some(code) = body
                .get("code")
                .and_then(Value::as_str)
                .filter(|c| !c.is_empty())
            {
                let message = body
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                return XinghuoError::with_status(code, reply.status, message);
            }
        }
    }
    XinghuoError::with_status("HTTP_ERROR", reply.status, "")
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ActivateReply {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub token: String,
}

pub(crate) async fn activate(
    site: &str,
    cdk: &str,
    prior_token: Option<&str>,
) -> Result<ActivateReply, XinghuoError> {
    let url = format!("{site}{ACTIVATE_PATH}");
    let body = json!({
        "cdk": cdk,
        "client": CLIENT_ID,
        "token": prior_token.unwrap_or_default(),
    });
    let reply = send(
        |client| client.post(&url).json(&body),
        false,
        ACTIVATE_PATH,
    )
    .await?;
    if reply.status != 200 {
        return Err(reply_error(&reply));
    }
    let parsed: ActivateReply = serde_json::from_str(&reply.body)
        .map_err(|_| XinghuoError::with_status("BAD_RESPONSE", reply.status, ""))?;
    if !parsed.ok || parsed.token.trim().is_empty() {
        return Err(XinghuoError::with_status("BAD_RESPONSE", reply.status, ""));
    }
    Ok(ActivateReply {
        token: parsed.token.trim().to_string(),
        ..parsed
    })
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct StatusReply {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub valid: Option<bool>,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub quota: Option<Quota>,
    #[serde(default)]
    pub balance_scope: Option<String>,
}

/// 401 / 403 表示密钥无效或已停用（KEY_INVALID）；其余错误交给调用方决定是否放行。
pub(crate) fn parse_status_reply(reply: &HttpReply) -> Result<StatusReply, XinghuoError> {
    match reply.status {
        200 => serde_json::from_str::<StatusReply>(&reply.body)
            .ok()
            .filter(|s| s.ok)
            .ok_or_else(|| XinghuoError::with_status("BAD_RESPONSE", 200, "")),
        401 | 403 => {
            let upstream = reply_error(reply);
            Err(XinghuoError::with_status(
                "KEY_INVALID",
                reply.status,
                upstream.code,
            ))
        }
        _ => Err(reply_error(reply)),
    }
}

/// GET /api/v1/plugin/status
pub(crate) async fn status(site: &str, token: &str) -> Result<StatusReply, XinghuoError> {
    let url = format!("{site}{STATUS_PATH}");
    let reply = send(
        |client| client.get(&url).bearer_auth(token),
        true,
        STATUS_PATH,
    )
    .await?;
    parse_status_reply(&reply)
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ClientApp {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub models: Option<Value>,
    #[serde(default, alias = "apiFormat")]
    pub api_format: Option<String>,
}

impl ClientApp {
    /// OpenCode 的模型列表：字符串数组。
    pub fn model_list(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        if let Some(Value::Array(items)) = &self.models {
            for id in items.iter().filter_map(Value::as_str).map(str::trim) {
                if !id.is_empty() && !out.iter().any(|m| m == id) {
                    out.push(id.to_string());
                }
            }
        }
        out
    }

    /// Claude Code 的模型映射：`{"opus": "...", "sonnet": "...", ...}`，空对象表示不写模型变量。
    pub fn model_map(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        if let Some(Value::Object(map)) = &self.models {
            for (role, model) in map {
                if let Some(model) = model.as_str().map(str::trim).filter(|m| !m.is_empty()) {
                    out.insert(role.clone(), model.to_string());
                }
            }
        }
        out
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ClientApps {
    #[serde(default)]
    pub claude: Option<ClientApp>,
    #[serde(
        default,
        rename = "claude-desktop",
        alias = "claude_desktop",
        alias = "claudeDesktop"
    )]
    pub claude_desktop: Option<ClientApp>,
    #[serde(default)]
    pub codex: Option<ClientApp>,
    #[serde(default)]
    pub opencode: Option<ClientApp>,
    #[serde(default)]
    pub gemini: Option<ClientApp>,
    #[serde(default)]
    pub grokbuild: Option<ClientApp>,
    #[serde(default)]
    pub openclaw: Option<ClientApp>,
    #[serde(default)]
    pub hermes: Option<ClientApp>,
    #[serde(default)]
    pub mcode: Option<ClientApp>,
    #[serde(default)]
    pub pi: Option<ClientApp>,
}

impl ClientApps {
    pub fn get(&self, app: ManagedApp) -> Option<&ClientApp> {
        match app {
            ManagedApp::Claude => self.claude.as_ref(),
            ManagedApp::ClaudeDesktop => self.claude_desktop.as_ref(),
            ManagedApp::Codex => self.codex.as_ref(),
            ManagedApp::Opencode => self.opencode.as_ref(),
            ManagedApp::Gemini => self.gemini.as_ref(),
            ManagedApp::GrokBuild => self.grokbuild.as_ref(),
            ManagedApp::OpenClaw => self.openclaw.as_ref(),
            ManagedApp::Hermes => self.hermes.as_ref(),
            ManagedApp::Mcode => self.mcode.as_ref(),
            ManagedApp::Pi => self.pi.as_ref(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ClientBalance {
    #[serde(default)]
    pub unit_label: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ClientConfig {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub apps: Option<ClientApps>,
    #[serde(default)]
    pub balance: Option<ClientBalance>,
    #[serde(default)]
    pub contact: Option<String>,
    #[serde(default)]
    pub shop_url: Option<String>,
}

pub(crate) fn parse_client_config(reply: &HttpReply) -> Option<ClientConfig> {
    if reply.status != 200 {
        return None;
    }
    serde_json::from_str::<ClientConfig>(&reply.body)
        .ok()
        .filter(|c| c.ok)
}

/// GET /api/v1/plugin/client-config。老后端 404 或请求失败时返回 None，由调用方用默认值。
pub(crate) async fn client_config(site: &str, token: &str) -> Option<ClientConfig> {
    let url = format!("{site}{CLIENT_CONFIG_PATH}");
    match send(
        |client| client.get(&url).bearer_auth(token),
        true,
        CLIENT_CONFIG_PATH,
    )
    .await
    {
        Ok(reply) => parse_client_config(&reply),
        Err(error) => {
            log::warn!("[xinghuo] 获取客户端配置失败，使用默认值: {error}");
            None
        }
    }
}

pub(crate) fn parse_models(reply: &HttpReply) -> Option<Vec<String>> {
    if reply.status != 200 {
        return None;
    }
    let body: Value = serde_json::from_str(&reply.body).ok()?;
    let mut out: Vec<String> = Vec::new();
    for id in body
        .get("data")?
        .as_array()?
        .iter()
        .filter_map(|m| m.get("id").and_then(Value::as_str))
        .map(str::trim)
    {
        if !id.is_empty() && !out.iter().any(|m| m == id) {
            out.push(id.to_string());
        }
    }
    Some(out)
}

/// GET /v1/models；保留上游错误以区分额度、权限与网络问题。
pub(crate) async fn checked_models(site: &str, token: &str) -> Result<Vec<String>, XinghuoError> {
    let url = format!("{site}{MODELS_PATH}");
    let reply = send(
        |client| client.get(&url).bearer_auth(token),
        true,
        MODELS_PATH,
    )
    .await?;
    if reply.status != 200 {
        return Err(reply_error(&reply));
    }
    parse_models(&reply).ok_or_else(|| XinghuoError::new("BAD_RESPONSE", "模型列表格式无效"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(status: u16, body: &str) -> HttpReply {
        HttpReply {
            status,
            body: body.to_string(),
        }
    }

    #[test]
    fn server_errors_and_html_pages_collapse_to_http_error() {
        for (status, body) in [
            (500, r#"{"ok":false,"code":"INTERNAL_ERROR","message":"x"}"#),
            (502, "<html>Bad Gateway</html>"),
            (403, ""),
        ] {
            let error = reply_error(&reply(status, body));
            assert_eq!(error.code, "HTTP_ERROR", "status {status}");
            assert_eq!(error.status, Some(status));
        }
    }

    #[test]
    fn status_unauthorized_means_key_invalid() {
        for status in [401, 403] {
            let error = parse_status_reply(&reply(
                status,
                r#"{"ok":false,"code":"KEY_NOT_FOUND","message":"api key 不存在或已被删除"}"#,
            ))
            .unwrap_err();
            assert_eq!(error.code, "KEY_INVALID");
            assert_eq!(error.message, "KEY_NOT_FOUND");
        }
        let limited = parse_status_reply(&reply(
            429,
            r#"{"ok":false,"code":"RATE_LIMITED","message":"x"}"#,
        ))
        .unwrap_err();
        assert_eq!(limited.code, "RATE_LIMITED");
    }

    #[test]
    fn status_success_reads_validity_and_quota() {
        let parsed = parse_status_reply(&reply(
            200,
            r#"{"ok":true,"valid":false,"balance_scope":"key_cap","plan":"","expires_at":"2026-12-31T15:59:59Z","quota":{"unit":"USD","total":10,"used":10,"remaining":0}}"#,
        ))
        .unwrap();
        assert_eq!(parsed.valid, Some(false));
        assert_eq!(parsed.expires_at.as_deref(), Some("2026-12-31T15:59:59Z"));
        assert_eq!(parsed.quota.unwrap().remaining, 0.0);
        assert_eq!(parsed.balance_scope.as_deref(), Some("key_cap"));
    }

    #[test]
    fn client_config_404_means_use_defaults() {
        assert!(parse_client_config(&reply(404, "404 page not found")).is_none());
    }

    #[test]
    fn client_config_reads_apps_models_and_labels() {
        let config = parse_client_config(&reply(
            200,
            r#"{"ok":true,"base_url":"https://ai.wfpromax.xyz","apps":{"claude":{"enabled":true,"models":{"opus":"gpt-5.5","sonnet":""}},"codex":{"enabled":true,"model":"gpt-5.5"},"opencode":{"enabled":false,"models":["gpt-5.5","gpt-5.5",""]}},"balance":{"remaining":88.5,"unit_label":"¥","scope":"shadow"},"contact":"QQ 1","shop_url":"https://shop.example"}"#,
        ))
        .unwrap();
        let apps = config.apps.unwrap();
        let claude = apps.get(ManagedApp::Claude).unwrap();
        assert!(claude.enabled);
        assert_eq!(
            claude.model_map(),
            BTreeMap::from([("opus".to_string(), "gpt-5.5".to_string())])
        );
        assert_eq!(
            apps.get(ManagedApp::Codex).unwrap().model.as_deref(),
            Some("gpt-5.5")
        );
        let opencode = apps.get(ManagedApp::Opencode).unwrap();
        assert!(apps.get(ManagedApp::ClaudeDesktop).is_none());
        assert!(!opencode.enabled);
        assert_eq!(opencode.model_list(), vec!["gpt-5.5".to_string()]);
        assert_eq!(config.balance.unwrap().unit_label.as_deref(), Some("¥"));
    }

    #[test]
    fn explicit_desktop_config_overrides_legacy_codex_access() {
        let apps: ClientApps = serde_json::from_value(serde_json::json!({
            "codex": {"enabled": true, "model": "codex-model"},
            "claude-desktop": {"enabled": false, "model": "desktop-model"}
        }))
        .unwrap();
        let desktop = apps.get(ManagedApp::ClaudeDesktop).unwrap();
        assert!(!desktop.enabled);
        assert_eq!(desktop.model.as_deref(), Some("desktop-model"));
        assert_eq!(
            serde_json::to_string(&ManagedApp::ClaudeDesktop).unwrap(),
            "\"claude-desktop\""
        );
    }

    #[test]
    fn client_config_reads_extra_platforms_and_absent_means_hidden() {
        let apps: ClientApps = serde_json::from_value(serde_json::json!({
            "gemini": {"enabled": true, "model": "gpt-5.5"},
            "openclaw": {"enabled": true, "models": ["gpt-5.5", "gpt-5.5-mini"]},
            "mcode": {"enabled": false, "model": "gpt-5.5"}
        }))
        .unwrap();
        assert_eq!(
            apps.get(ManagedApp::Gemini).unwrap().model.as_deref(),
            Some("gpt-5.5")
        );
        assert_eq!(
            apps.get(ManagedApp::OpenClaw).unwrap().model_list(),
            vec!["gpt-5.5".to_string(), "gpt-5.5-mini".to_string()]
        );
        assert!(!apps.get(ManagedApp::Mcode).unwrap().enabled);
        assert!(apps.get(ManagedApp::GrokBuild).is_none());
        assert!(apps.get(ManagedApp::Hermes).is_none());
        assert!(apps.get(ManagedApp::Pi).is_none());
    }

    #[test]
    fn models_are_deduplicated_in_order() {
        let models = parse_models(&reply(
            200,
            r#"{"object":"list","data":[{"id":"gpt-5.5"},{"id":"gpt-5.5-mini"},{"id":"gpt-5.5"}]}"#,
        ))
        .unwrap();
        assert_eq!(models, vec!["gpt-5.5", "gpt-5.5-mini"]);
        assert!(parse_models(&reply(401, "{}")).is_none());
    }
}
