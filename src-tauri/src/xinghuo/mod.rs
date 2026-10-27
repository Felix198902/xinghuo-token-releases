//! 星火激活：CDK 无设备绑定地兑换 SK，SK 作为 Bearer 凭据，
//! 写盘后为各支持平台建立固定 ID 的供应商并切换过去。
//!
//! 三条硬规则：
//! 1. 凭据第一时间写盘（activation.json + wallet.json），再做别的；
//! 2. 不绑定设备，CDK/SK 最终共用状态与额度查询；
//! 3. 顾客可见文案只写现象、错误码和联系方式，日志不得记录完整凭据。

mod api;
pub mod diagnostics;
mod providers;
pub mod restore;
mod storage;
mod usage;

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::app_config::AppType;
use crate::store::AppState;

pub use providers::{apply_providers, deactivate_providers, AppliedApp, ApplyPlan};
pub use storage::DesktopConnection;
pub use storage::{ActivationRecord, Quota};
pub use usage::{is_usage_script, query_usage, usage_script, USAGE_TEMPLATE_TYPE};

/// 站点根地址。Claude Code 直接用它；Codex / OpenCode 在后面拼 `/v1`。
/// 前端同名常量在 `src/config/brand.ts`，改这里时一起改。
pub const SITE_URL: &str = "https://ai.wfpromax.xyz";

/// 实际使用的服务端地址。debug 构建可以用环境变量 `XINGHUO_SITE_URL` 指向本地或
/// 测试环境（端到端验证用）；release 构建固定为 [`SITE_URL`]，不受环境变量影响。
pub fn site_url() -> String {
    #[cfg(debug_assertions)]
    {
        if let Ok(value) = std::env::var("XINGHUO_SITE_URL") {
            let value = value.trim().trim_end_matches('/');
            if !value.is_empty() {
                return value.to_string();
            }
        }
    }
    SITE_URL.to_string()
}
/// 客户端里星火供应商的显示名。
pub const PROVIDER_NAME: &str = "星火无限token";
/// 星火供应商在列表里使用的图标名（见 `src/icons/extracted`）。
pub const PROVIDER_ICON: &str = "xinghuo";
/// 服务端未下发配置时 Codex / OpenCode 使用的默认模型。
pub const DEFAULT_MODEL: &str = "gpt-5.5";
/// 服务端未下发时的系统管理员联系方式（与前端 brand.ts 一致）。
pub const DEFAULT_CONTACT: &str = "QQ 169106354 / 微信 US666377";

/// 激活页负责配置的应用。固定 ID 让重复激活只会更新同一个供应商，
/// 而不是像 deep link 导入那样每次新建一个「名称-时间戳」。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ManagedApp {
    Claude,
    #[serde(rename = "claude-desktop")]
    ClaudeDesktop,
    Codex,
    Opencode,
    Gemini,
    GrokBuild,
    OpenClaw,
    Hermes,
    Mcode,
    Pi,
}

impl ManagedApp {
    pub const ALL: [ManagedApp; 10] = [
        ManagedApp::Claude,
        ManagedApp::ClaudeDesktop,
        ManagedApp::Codex,
        ManagedApp::Opencode,
        ManagedApp::Gemini,
        ManagedApp::GrokBuild,
        ManagedApp::OpenClaw,
        ManagedApp::Hermes,
        ManagedApp::Mcode,
        ManagedApp::Pi,
    ];

    /// 扩展平台：只在服务端 client-config 明确下发时才可接入，老后端不下发即隐藏。
    pub const EXTRA: [ManagedApp; 6] = [
        ManagedApp::Gemini,
        ManagedApp::GrokBuild,
        ManagedApp::OpenClaw,
        ManagedApp::Hermes,
        ManagedApp::Mcode,
        ManagedApp::Pi,
    ];

    pub fn app_type(self) -> AppType {
        match self {
            ManagedApp::Claude => AppType::Claude,
            ManagedApp::ClaudeDesktop => AppType::ClaudeDesktop,
            ManagedApp::Codex => AppType::Codex,
            ManagedApp::Opencode => AppType::OpenCode,
            ManagedApp::Gemini => AppType::Gemini,
            ManagedApp::GrokBuild => AppType::GrokBuild,
            ManagedApp::OpenClaw => AppType::OpenClaw,
            ManagedApp::Hermes => AppType::Hermes,
            ManagedApp::Mcode => AppType::Mcode,
            ManagedApp::Pi => AppType::Pi,
        }
    }

    pub fn provider_id(self) -> &'static str {
        match self {
            ManagedApp::Claude => "xinghuo-claude",
            ManagedApp::ClaudeDesktop => "xinghuo-claude-desktop",
            ManagedApp::Codex => "xinghuo-codex",
            ManagedApp::Opencode => "xinghuo-opencode",
            ManagedApp::Gemini => "xinghuo-gemini",
            ManagedApp::GrokBuild => "xinghuo-grokbuild",
            ManagedApp::OpenClaw => "xinghuo-openclaw",
            ManagedApp::Hermes => "xinghuo-hermes",
            ManagedApp::Mcode => "xinghuo-mcode",
            ManagedApp::Pi => "xinghuo-pi",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ManagedApp::Claude => "claude",
            ManagedApp::ClaudeDesktop => "claude-desktop",
            ManagedApp::Codex => "codex",
            ManagedApp::Opencode => "opencode",
            ManagedApp::Gemini => "gemini",
            ManagedApp::GrokBuild => "grokbuild",
            ManagedApp::OpenClaw => "openclaw",
            ManagedApp::Hermes => "hermes",
            ManagedApp::Mcode => "mcode",
            ManagedApp::Pi => "pi",
        }
    }
}

/// 回传给前端的错误。`code` 决定顾客看到的文案，`status` 用来拼 `XH-4<状态码>`，
/// `message` 只作诊断，不直接上屏。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct XinghuoError {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    pub message: String,
}

impl XinghuoError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            status: None,
            message: message.into(),
        }
    }

    pub fn with_status(code: &str, status: u16, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            status: Some(status),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for XinghuoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.status {
            Some(status) => write!(f, "{} (HTTP {status}): {}", self.code, self.message),
            None => write!(f, "{}: {}", self.code, self.message),
        }
    }
}

/// 给前端展示的激活状态。密钥只回传打码后的形式。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivationView {
    pub activated: bool,
    pub restore_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub masked_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota: Option<Quota>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance_scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid: Option<bool>,
    pub apps: Vec<ManagedApp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shop_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activated_at: Option<i64>,
    pub has_wallet: bool,
    pub desktop_connection_configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop_masked_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop_api_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop_model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivateResult {
    pub view: ActivationView,
    pub apps: Vec<AppliedApp>,
    /// 续费：拿回的是本机已有的同一把密钥，各工具配置保持不动。
    pub renewed: bool,
    pub already_connected: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeactivateResult {
    pub view: ActivationView,
    pub apps: Vec<AppliedApp>,
}

/// 已经写盘的密钥，以及它相对本机原有激活的关系。
#[derive(Debug, Clone)]
pub struct KeyGrant {
    pub record: ActivationRecord,
    /// 与本机当前激活是同一把密钥（续费或重复激活）。
    pub renewal: bool,
}

pub(crate) fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub(crate) fn is_api_key(input: &str) -> bool {
    input.starts_with("sk-")
}

/// `sk-abc…wxyz`：保留前 6 位和后 4 位，足够顾客认出是哪一把。
pub(crate) fn mask_key(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() <= 12 {
        let head: String = chars.iter().take(3).collect();
        return format!("{head}…");
    }
    let head: String = chars.iter().take(6).collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}…{tail}")
}

fn view_of(record: Option<&ActivationRecord>) -> ActivationView {
    let has_wallet = storage::load_wallet().is_some_and(|w| !w.token.is_empty());
    let Some(record) = record.filter(|r| !r.token.is_empty()) else {
        return ActivationView {
            has_wallet,
            restore_available: restore::available(),
            ..Default::default()
        };
    };
    ActivationView {
        activated: true,
        restore_available: restore::available() || !record.previous.is_empty(),
        masked_key: Some(mask_key(&record.token)),
        source: Some(record.source.clone()),
        plan: record.plan.clone(),
        expires_at: record.expires_at.clone(),
        quota: record.quota.clone(),
        balance_scope: record.balance_scope.clone(),
        valid: record.valid,
        apps: record.apps.clone(),
        unit_label: record.unit_label.clone(),
        contact: record.contact.clone(),
        shop_url: record.shop_url.clone(),
        activated_at: Some(record.activated_at),
        has_wallet,
        desktop_connection_configured: record.desktop_connection.is_some(),
        desktop_masked_key: record
            .desktop_connection
            .as_ref()
            .map(|c| mask_key(&c.token)),
        desktop_api_format: record
            .desktop_connection
            .as_ref()
            .map(|c| c.api_format.clone()),
        desktop_model: record
            .desktop_connection
            .as_ref()
            .and_then(|c| c.model.clone()),
    }
}

/// 读本机激活状态（不联网）。
pub fn current_view() -> ActivationView {
    view_of(storage::load_activation().as_ref())
}

/// 第一步：校验输入、取得可用 SK，并在返回前写盘。
/// CDK 通过无设备绑定的兑换接口换 SK，随后与直接输入的 SK 走相同状态与额度查询。
/// 相同 CDK 在本机重复提交时直接复用激活记录，避免重复兑换触发限流。
pub async fn obtain_key(input: &str) -> Result<KeyGrant, XinghuoError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(XinghuoError::new("INPUT_EMPTY", "激活码或密钥为空"));
    }

    let input_hash = format!("{:x}", Sha256::digest(input.as_bytes()));
    let existing = storage::load_activation().filter(|r| !r.token.is_empty());
    if let Some(record) = existing.clone().filter(|record| {
        record.source == "cdk" && record.input_hash.as_deref() == Some(input_hash.as_str())
    }) {
        log::info!("[xinghuo] reusing activation for identical CDK input");
        return Ok(KeyGrant {
            record,
            renewal: true,
        });
    }
    let mut record = existing.clone().unwrap_or_default();

    if is_api_key(input) {
        record.token = input.to_string();
        record.source = "key".to_string();
    } else {
        let prior_token = existing
            .as_ref()
            .map(|r| r.token.clone())
            .or_else(|| storage::load_wallet().map(|w| w.token))
            .filter(|t| !t.is_empty());
        let reply = api::activate(&site_url(), input, prior_token.as_deref()).await?;
        record.token = reply.token;
        record.source = "cdk".to_string();
    }
    record.input_hash = Some(input_hash);
    log::info!(
        "[xinghuo] activation started: source={}, device_binding=false",
        record.source
    );

    let status = match api::status(&site_url(), &record.token).await {
        Ok(status) => {
            log::info!("[xinghuo] activation status query succeeded");
            Some(status)
        }
        Err(error) if error.code == "KEY_INVALID" => return Err(error),
        Err(error) => {
            log::warn!("[xinghuo] 密钥探活失败，按规则放行: {error}");
            None
        }
    };
    if let Some(status) = status {
        record.plan = status.plan.filter(|p| !p.is_empty());
        record.expires_at = status.expires_at;
        record.quota = status.quota;
        record.balance_scope = status.balance_scope;
        record.valid = status.valid;
    } else {
        record.plan = None;
        record.expires_at = None;
        record.quota = None;
        record.balance_scope = None;
        record.valid = None;
    }

    let renewal = existing
        .as_ref()
        .is_some_and(|previous| previous.token == record.token);
    let now = now_ms();
    if existing.is_none() {
        record.activated_at = now;
    }
    record.updated_at = now;

    // 硬规则 1：先落盘，再做任何别的事。
    storage::save_wallet_token(&record.token)
        .and_then(|_| storage::save_activation(&record))
        .map_err(|e| XinghuoError::new("STORAGE", e.to_string()))?;

    Ok(KeyGrant { record, renewal })
}

/// 一键接入使用本机保存的密钥，不向前端暴露完整凭据。
pub fn active_record() -> Result<ActivationRecord, XinghuoError> {
    storage::load_activation()
        .filter(|record| !record.token.is_empty())
        .ok_or_else(|| XinghuoError::new("NOT_ACTIVATED", "请先激活星火服务"))
}

/// 在联网发现模型和写配置前检查，已接入时只恢复网关运行。
pub fn desktop_already_connected(
    state: &AppState,
    record: &ActivationRecord,
) -> Option<ActivateResult> {
    use crate::services::ProviderService;
    let app = ManagedApp::ClaudeDesktop;
    // 旧版单模型过滤需要一次迁移，恢复桌面内切换。
    if record
        .desktop_connection
        .as_ref()
        .is_some_and(|c| c.model.is_some())
        || ProviderService::current(state, app.app_type()).ok()? != app.provider_id()
    {
        return None;
    }
    let provider = state
        .db
        .get_provider_by_id(app.provider_id(), app.as_str())
        .ok()??;
    let token = desktop_input_token(record, "");
    if provider.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"].as_str() != Some(token)
        || provider.settings_config["env"]["ANTHROPIC_BASE_URL"].as_str()
            != Some(format!("{}/v1", site_url().trim_end_matches('/')).as_str())
        || !crate::claude_desktop_config::is_proxy_provider_applied(&state.db, &provider).ok()?
    {
        return None;
    }
    // 网关停了不能提示成功；启动失败交由正常接入流程报告。
    tauri::async_runtime::block_on(state.proxy_service.start()).ok()?;
    Some(ActivateResult {
        view: view_of(Some(record)),
        apps: vec![AppliedApp::from_result(app, Ok(Vec::new()))],
        renewed: false,
        already_connected: true,
    })
}

/// 只配置所选工具；恢复原配置后再次点击也会重新写入固定星火接口。
pub fn finish_use_app(
    state: &AppState,
    record: ActivationRecord,
    mut plan: ApplyPlan,
    app: ManagedApp,
) -> Result<ActivateResult, XinghuoError> {
    if !plan.apps.contains(&app) {
        return Err(XinghuoError::new(
            "APP_UNAVAILABLE",
            "当前服务暂不支持此工具",
        ));
    }
    plan.apps = vec![app];
    Ok(finish_activation(
        state,
        KeyGrant {
            record,
            renewal: false,
        },
        plan,
    ))
}

/// 第二步：按服务端下发的客户端配置决定要配哪些工具、用什么模型。
/// 接口不存在（老后端 404）或请求失败时用内置默认值：四个工具全开。
pub async fn fetch_plan(token: &str) -> ApplyPlan {
    let config = api::client_config(&site_url(), token).await;
    let mut plan = ApplyPlan::defaults();
    plan.claude_desktop_api_format =
        desktop_api_format(config.as_ref().and_then(|c| c.apps.as_ref())).into();

    if let Some(config) = config.as_ref() {
        if let Some(apps) = config.apps.as_ref() {
            plan.apps = ManagedApp::ALL
                .into_iter()
                .filter(|app| {
                    if *app == ManagedApp::ClaudeDesktop {
                        apps.claude_desktop
                            .as_ref()
                            .is_none_or(|entry| entry.enabled)
                    } else {
                        apps.get(*app).is_some_and(|entry| entry.enabled)
                    }
                })
                .collect();
            if let Some(model) = apps
                .codex
                .as_ref()
                .and_then(|codex| codex.model.clone())
                .filter(|m| !m.trim().is_empty())
            {
                plan.codex_model = model;
            }
            if let Some(models) = apps.opencode.as_ref().map(|o| o.model_list()) {
                if !models.is_empty() {
                    plan.opencode_models = models;
                }
            }
            if let Some(models) = apps.claude.as_ref().map(|c| c.model_map()) {
                plan.claude_models = models;
            }
            for app in ManagedApp::EXTRA {
                let Some(entry) = apps.get(app) else { continue };
                let mut models = entry.model_list();
                if models.is_empty() {
                    models = entry
                        .model
                        .as_deref()
                        .map(str::trim)
                        .filter(|model| !model.is_empty())
                        .map(|model| vec![model.to_string()])
                        .unwrap_or_default();
                }
                if !models.is_empty() {
                    plan.extra_models.insert(app, models);
                }
            }
            plan.claude_desktop_model = apps
                .claude_desktop
                .as_ref()
                .and_then(|desktop| desktop.model.as_deref())
                .map(str::trim)
                .filter(|model| !model.is_empty())
                .map(str::to_string);
        }
        plan.unit_label = config
            .balance
            .as_ref()
            .and_then(|b| b.unit_label.clone())
            .filter(|l| !l.trim().is_empty());
        plan.contact = config.contact.clone().filter(|c| !c.trim().is_empty());
        plan.shop_url = config
            .shop_url
            .clone()
            .filter(|u| u.starts_with("https://"));
    }

    // OpenCode 与扩展平台的模型表：服务端没给就拉 /v1/models，再不行用默认模型。
    let opencode_from_config = config
        .as_ref()
        .and_then(|c| c.apps.as_ref())
        .and_then(|apps| apps.opencode.as_ref())
        .is_some_and(|o| !o.model_list().is_empty());
    let desktop_config = config
        .as_ref()
        .and_then(|c| c.apps.as_ref())
        .and_then(|apps| apps.claude_desktop.as_ref());
    let desktop_enabled = plan.apps.contains(&ManagedApp::ClaudeDesktop);
    let extra_needs_discovery = ManagedApp::EXTRA
        .into_iter()
        .any(|app| plan.apps.contains(&app) && !plan.extra_models.contains_key(&app));
    if desktop_enabled
        || (plan.apps.contains(&ManagedApp::Opencode) && !opencode_from_config)
        || extra_needs_discovery
    {
        let models = match api::checked_models(&site_url(), token).await {
            Ok(models) => models,
            Err(error) => {
                log::warn!(
                    "[xinghuo] 模型发现失败: code={}, status={:?}",
                    error.code,
                    error.status
                );
                if desktop_enabled {
                    plan.claude_desktop_model_error = Some(error);
                }
                Vec::new()
            }
        };
        if desktop_enabled {
            plan.claude_desktop_models = resolve_desktop_models(desktop_config, &models);
            log::info!(
                "[xinghuo] Claude Desktop 模型发现: 接入 {} 个",
                plan.claude_desktop_models.len()
            );
            // 单模型配置也必须属于当前密钥可用模型，不能绕过发现失败。
            plan.claude_desktop_model = None;
        }
        if plan.apps.contains(&ManagedApp::Opencode) && !opencode_from_config && !models.is_empty()
        {
            plan.opencode_models = models.clone();
        }
        if !models.is_empty() {
            for app in ManagedApp::EXTRA {
                if plan.apps.contains(&app) && !plan.extra_models.contains_key(&app) {
                    plan.extra_models.insert(app, models.clone());
                }
            }
        }
    }

    // 桌面独立配置不能被主套餐续费或再次一键接入覆盖。
    if let Some(connection) = storage::load_activation().and_then(|r| r.desktop_connection) {
        let models = match api::checked_models(&site_url(), &connection.token).await {
            Ok(models) => {
                plan.claude_desktop_model_error = None;
                models
            }
            Err(error) => {
                log::warn!(
                    "[xinghuo] 独立 Claude Desktop 模型发现失败: code={}, status={:?}",
                    error.code,
                    error.status
                );
                plan.claude_desktop_model_error = Some(error);
                Vec::new()
            }
        };
        apply_desktop_connection_to_plan(&mut plan, &connection, &models);
    }
    plan
}

fn apply_desktop_connection_to_plan(
    plan: &mut ApplyPlan,
    connection: &DesktopConnection,
    models: &[String],
) {
    if !plan.apps.contains(&ManagedApp::ClaudeDesktop) {
        plan.apps.push(ManagedApp::ClaudeDesktop);
    }
    plan.claude_desktop_token = Some(connection.token.clone());
    plan.claude_desktop_api_format = connection.api_format.clone();
    plan.claude_desktop_model = None;
    plan.claude_desktop_models = resolve_desktop_models(None, models);
}

/// 只读发现当前用户密钥的全部可用模型，不发送付费推理请求。
pub async fn discover_desktop_models(input: &str) -> Result<Vec<String>, XinghuoError> {
    let record = active_record()?;
    let token = desktop_input_token(&record, input);
    let models = api::checked_models(&site_url(), token)
        .await
        .map_err(|error| {
            log::warn!(
                "[xinghuo] Claude Desktop 手动读取模型失败: code={}, status={:?}",
                error.code,
                error.status
            );
            error
        })?;
    let total = models.len();
    let models = resolve_desktop_models(None, &models);
    log::info!("[xinghuo] Claude Desktop 手动读取模型: 接入 {total} 个");
    if models.is_empty() {
        return Err(XinghuoError::new(
            "DESKTOP_MODELS",
            "此密钥没有可用模型，原配置未修改",
        ));
    }
    Ok(models)
}

pub async fn prepare_desktop_connection(
    input: &str,
    model: Option<String>,
) -> Result<(ActivationRecord, DesktopConnection, ApplyPlan), XinghuoError> {
    let record = active_record()?;
    let models = discover_desktop_models(input).await?;
    let model = model
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if model
        .as_ref()
        .is_some_and(|selected| !models.contains(selected))
    {
        return Err(XinghuoError::new(
            "DESKTOP_MODELS",
            "所选模型不属于当前密钥，原配置未修改",
        ));
    }
    let token = desktop_input_token(&record, input);
    let config = api::client_config(&site_url(), token).await;
    // 协议由后台统一选择；配置接口不可用时保留同一密钥的已知协议。
    let api_format = match config.as_ref() {
        Some(config) => desktop_api_format(config.apps.as_ref()),
        None => record
            .desktop_connection
            .as_ref()
            .filter(|connection| connection.token == token)
            .map(|connection| connection.api_format.as_str())
            .filter(|format| matches!(*format, "anthropic" | "openai_chat" | "openai_responses"))
            .unwrap_or("openai_chat"),
    };
    let connection = DesktopConnection {
        token: token.to_string(),
        api_format: api_format.into(),
        // 模型由 Claude Desktop 内部选择，保存完整可用目录。
        model: None,
    };
    let mut plan = ApplyPlan::defaults();
    apply_desktop_connection_to_plan(&mut plan, &connection, &models);
    plan.apps = vec![ManagedApp::ClaudeDesktop];
    Ok((record, connection, plan))
}

fn desktop_input_token<'a>(record: &'a ActivationRecord, input: &'a str) -> &'a str {
    if !input.trim().is_empty() {
        input.trim()
    } else {
        record
            .desktop_connection
            .as_ref()
            .map(|c| c.token.as_str())
            .unwrap_or(&record.token)
    }
}

/// 只修改桌面配置；保存成功后才记住独立凭据，其他工具和主套餐不变。
pub fn finish_desktop_connection(
    state: &AppState,
    mut record: ActivationRecord,
    connection: DesktopConnection,
    plan: ApplyPlan,
) -> Result<ActivateResult, XinghuoError> {
    let applied = apply_providers(state, &record.token.clone(), &plan, false, &mut record);
    if let Some(failed) = applied.iter().find(|app| !app.ok) {
        return Err(XinghuoError::new(
            "DESKTOP_CONFIG",
            failed.error.clone().unwrap_or_default(),
        ));
    }
    record.desktop_connection = Some(connection);
    if !record.apps.contains(&ManagedApp::ClaudeDesktop) {
        record.apps.push(ManagedApp::ClaudeDesktop);
    }
    record.updated_at = now_ms();
    storage::save_activation(&record).map_err(|e| XinghuoError::new("STORAGE", e.to_string()))?;
    Ok(ActivateResult {
        view: view_of(Some(&record)),
        apps: applied,
        renewed: false,
        already_connected: false,
    })
}

fn desktop_api_format(apps: Option<&api::ClientApps>) -> &'static str {
    match apps
        .and_then(|apps| apps.claude_desktop.as_ref())
        .and_then(|entry| entry.api_format.as_deref())
    {
        Some("anthropic") => "anthropic",
        Some("openai_chat") => "openai_chat",
        Some("openai_responses") => "openai_responses",
        _ if apps
            .and_then(|apps| apps.claude.as_ref())
            .is_some_and(|entry| entry.enabled) =>
        {
            "anthropic"
        }
        // 老后端未声明协议时走通用 Chat 桥接；Responses 仅在明确下发时启用。
        _ => "openai_chat",
    }
}

fn resolve_desktop_models(_config: Option<&api::ClientApp>, available: &[String]) -> Vec<String> {
    available.to_vec()
}

/// 第三步（阻塞，放在 spawn_blocking 里调）：建立/更新供应商并切换，记下结果。
pub fn finish_activation(state: &AppState, grant: KeyGrant, plan: ApplyPlan) -> ActivateResult {
    let KeyGrant {
        mut record,
        renewal,
    } = grant;
    let token = record.token.clone();
    let applied = apply_providers(state, &token, &plan, renewal, &mut record);

    if !renewal
        && applied
            .iter()
            .any(|app| app.app == ManagedApp::ClaudeDesktop && app.ok)
    {
        if let Some(connection) = record.desktop_connection.as_mut() {
            connection.model = None;
        }
    }

    let mut apps: BTreeSet<ManagedApp> = record.apps.iter().copied().collect();
    apps.extend(applied.iter().filter(|a| a.ok).map(|a| a.app));
    record.apps = apps.into_iter().collect();
    if plan.unit_label.is_some() {
        record.unit_label = plan.unit_label.clone();
    }
    if plan.contact.is_some() {
        record.contact = plan.contact.clone();
    }
    if plan.shop_url.is_some() {
        record.shop_url = plan.shop_url.clone();
    }
    record.updated_at = now_ms();
    if let Err(e) = storage::save_activation(&record) {
        // 密钥在 obtain_key 里已经落盘，这里只是补记配置结果。
        log::warn!("[xinghuo] 写入激活记录失败: {e}");
    }

    ActivateResult {
        view: view_of(Some(&record)),
        apps: applied,
        renewed: renewal,
        already_connected: false,
    }
}

/// 联网刷新套餐、到期和余额。密钥被停用（401 / 403）时记为无效。
pub async fn refresh_status() -> Result<ActivationView, XinghuoError> {
    let Some(mut record) = storage::load_activation().filter(|r| !r.token.is_empty()) else {
        return Ok(view_of(None));
    };
    match api::status(&site_url(), &record.token).await {
        Ok(status) => {
            record.valid = status.valid;
            if let Some(plan) = status.plan.filter(|p| !p.is_empty()) {
                record.plan = Some(plan);
            }
            record.expires_at = status.expires_at;
            record.quota = status.quota;
            record.balance_scope = status.balance_scope;
        }
        Err(error) if error.code == "KEY_INVALID" => {
            record.valid = Some(false);
        }
        Err(error) => return Err(error),
    }
    record.updated_at = now_ms();
    storage::save_activation(&record).map_err(|e| XinghuoError::new("STORAGE", e.to_string()))?;
    Ok(view_of(Some(&record)))
}

/// 退出激活（阻塞）：各工具切回激活前的供应商，删除星火供应商；钱包保留。
pub fn deactivate(state: &AppState) -> DeactivateResult {
    let record = storage::load_activation().unwrap_or_default();
    let applied = deactivate_providers(state, &record);
    if applied.iter().all(|a| a.ok) {
        if let Err(e) = storage::clear_activation() {
            log::warn!("[xinghuo] 清除激活记录失败: {e}");
        }
        return DeactivateResult {
            view: view_of(None),
            apps: applied,
        };
    }
    DeactivateResult {
        view: view_of(Some(&record)),
        apps: applied,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_override_is_scoped_and_keeps_all_available_models() {
        let mut plan = ApplyPlan::defaults();
        let connection = DesktopConnection {
            token: "sk-desktop-only".into(),
            api_format: "openai_chat".into(),
            model: Some("claude-opus-4-7".into()),
        };
        apply_desktop_connection_to_plan(
            &mut plan,
            &connection,
            &[
                "gpt-5.5".into(),
                "deepseek-v4-pro".into(),
                "claude-opus-4-7".into(),
                "claude-sonnet-5".into(),
            ],
        );
        assert_eq!(
            plan.claude_desktop_token.as_deref(),
            Some("sk-desktop-only")
        );
        assert_eq!(
            plan.claude_desktop_models,
            vec![
                "gpt-5.5",
                "deepseek-v4-pro",
                "claude-opus-4-7",
                "claude-sonnet-5"
            ]
        );
        assert_eq!(plan.codex_model, DEFAULT_MODEL);
        assert_eq!(plan.claude_desktop_api_format, "openai_chat");
        apply_desktop_connection_to_plan(&mut plan, &connection, &["deepseek-v4-pro".into()]);
        assert_eq!(plan.claude_desktop_models, vec!["deepseek-v4-pro"]);
        assert!(plan.claude_desktop_model.is_none());
    }

    #[test]
    fn desktop_view_redacts_saved_credentials_and_blank_input_reuses_them() {
        let record = ActivationRecord {
            token: "sk-primary-private-1234".into(),
            desktop_connection: Some(DesktopConnection {
                token: "sk-desktop-private-5678".into(),
                api_format: "openai_chat".into(),
                model: Some("claude-opus-4-7".into()),
            }),
            ..Default::default()
        };
        assert_eq!(desktop_input_token(&record, ""), "sk-desktop-private-5678");
        assert_eq!(
            desktop_input_token(&record, " sk-replacement "),
            "sk-replacement"
        );
        let view = serde_json::to_string(&view_of(Some(&record))).unwrap();
        assert!(!view.contains(&record.token));
        assert!(!view.contains("sk-desktop-private-5678"));
        assert!(view.contains("claude-opus-4-7"));
    }

    #[test]
    fn desktop_uses_chat_when_messages_dispatch_is_disabled() {
        let apps: api::ClientApps = serde_json::from_value(serde_json::json!({
            "claude": {"enabled":false}, "codex":{"enabled":true,"model":"gpt-5.5"}
        }))
        .unwrap();
        assert_eq!(desktop_api_format(Some(&apps)), "openai_chat");
        let native: api::ClientApps =
            serde_json::from_value(serde_json::json!({"claude":{"enabled":true}})).unwrap();
        assert_eq!(desktop_api_format(Some(&native)), "anthropic");
        assert_eq!(desktop_api_format(None), "openai_chat");
        let explicit: api::ClientApps = serde_json::from_value(
            serde_json::json!({"claude-desktop":{"enabled":true,"api_format":"openai_responses"}}),
        )
        .unwrap();
        assert_eq!(desktop_api_format(Some(&explicit)), "openai_responses");
    }

    #[test]
    fn desktop_discovers_all_models_without_single_model_fallback() {
        let available = vec![
            "gpt-5.5".into(),
            "deepseek-v4-pro".into(),
            "hermes-3".into(),
            "claude-sonnet-5".into(),
            "claude-opus-4-8".into(),
        ];
        assert_eq!(resolve_desktop_models(None, &available), available);
        assert_eq!(
            resolve_desktop_models(None, &["deepseek-v4-pro".into()]),
            vec!["deepseek-v4-pro"]
        );
        assert!(resolve_desktop_models(None, &[]).is_empty());
    }

    #[test]
    fn desktop_catalog_ignores_legacy_allowlist_and_checks_key_permissions() {
        let config: api::ClientApp = serde_json::from_value(serde_json::json!({
            "enabled": true, "models": ["claude-opus-5", "claude-unavailable"]
        }))
        .unwrap();
        let available = vec![
            "deepseek-v4-pro".into(),
            "claude-opus-5".into(),
            "claude-sonnet-5-5".into(),
            "gpt-5.5".into(),
        ];
        assert_eq!(resolve_desktop_models(Some(&config), &available), available);
        assert!(resolve_desktop_models(Some(&config), &[]).is_empty());
    }

    #[test]
    fn api_key_detection_only_matches_sk_prefix() {
        assert!(is_api_key("sk-abc"));
        assert!(!is_api_key("XH-ABCDE-FGHJK-MNPQR"));
        assert!(!is_api_key("SK-abc"));
    }

    #[test]
    fn mask_key_keeps_head_and_tail_only() {
        assert_eq!(mask_key("sk-1234567890abcdef"), "sk-123…cdef");
        assert_eq!(mask_key("sk-short"), "sk-…");
    }

    #[test]
    fn managed_apps_use_fixed_provider_ids() {
        let ids: Vec<_> = ManagedApp::ALL.iter().map(|a| a.provider_id()).collect();
        assert_eq!(
            ids,
            [
                "xinghuo-claude",
                "xinghuo-claude-desktop",
                "xinghuo-codex",
                "xinghuo-opencode",
                "xinghuo-gemini",
                "xinghuo-grokbuild",
                "xinghuo-openclaw",
                "xinghuo-hermes",
                "xinghuo-mcode",
                "xinghuo-pi"
            ]
        );
    }

    #[test]
    fn managed_app_ids_match_frontend_strings() {
        for app in ManagedApp::ALL {
            let json = serde_json::to_value(app).unwrap();
            assert_eq!(json.as_str(), Some(app.as_str()));
            let parsed: ManagedApp = serde_json::from_value(json).unwrap();
            assert_eq!(parsed, app);
        }
    }

    #[test]
    fn error_serializes_code_status_and_message() {
        let error = XinghuoError::with_status("HTTP_ERROR", 502, "");
        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(json["code"], "HTTP_ERROR");
        assert_eq!(json["status"], 502);
        let without_status = serde_json::to_value(XinghuoError::new("NETWORK", "timeout")).unwrap();
        assert!(without_status.get("status").is_none());
    }
}
