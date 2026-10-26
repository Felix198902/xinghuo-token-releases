//! 把一把密钥落成各工具的供应商配置：固定 ID、写入后切换过去；退出激活时
//! 切回激活前的供应商再删除。

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{json, Map, Value};

use super::{
    now_ms, ActivationRecord, ManagedApp, XinghuoError, DEFAULT_MODEL, PROVIDER_ICON, PROVIDER_NAME,
};
use crate::error::AppError;
use crate::provider::{ClaudeDesktopMode, ClaudeDesktopModelRoute, Provider, ProviderMeta};
use crate::services::ProviderService;
use crate::store::AppState;

/// 本次要配置哪些工具、用什么模型。
#[derive(Debug, Clone, PartialEq)]
pub struct ApplyPlan {
    pub apps: Vec<ManagedApp>,
    pub codex_model: String,
    pub claude_desktop_model: Option<String>,
    pub claude_desktop_models: Vec<String>,
    pub claude_desktop_model_error: Option<XinghuoError>,
    pub claude_desktop_api_format: String,
    pub claude_desktop_token: Option<String>,
    pub opencode_models: Vec<String>,
    /// Claude Code 的模型映射（opus / sonnet / haiku / model），空表示不写模型变量。
    pub claude_models: BTreeMap<String, String>,
    /// 扩展平台（gemini / grokbuild / openclaw / hermes / mcode / pi）的模型表，
    /// 首个为默认模型；服务端和模型发现都没给时回退到 [`DEFAULT_MODEL`]。
    pub extra_models: BTreeMap<ManagedApp, Vec<String>>,
    pub unit_label: Option<String>,
    pub contact: Option<String>,
    pub shop_url: Option<String>,
}

impl ApplyPlan {
    /// 服务端没有下发客户端配置时的默认值：四个工具全开，扩展平台不下发不接入。
    pub fn defaults() -> Self {
        Self {
            apps: vec![
                ManagedApp::Claude,
                ManagedApp::ClaudeDesktop,
                ManagedApp::Codex,
                ManagedApp::Opencode,
            ],
            codex_model: DEFAULT_MODEL.to_string(),
            claude_desktop_model: None,
            claude_desktop_models: Vec::new(),
            claude_desktop_model_error: None,
            claude_desktop_api_format: "anthropic".into(),
            claude_desktop_token: None,
            opencode_models: vec![DEFAULT_MODEL.to_string()],
            claude_models: BTreeMap::new(),
            extra_models: BTreeMap::new(),
            unit_label: None,
            contact: None,
            shop_url: None,
        }
    }
}

/// 单个工具的处理结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedApp {
    pub app: ManagedApp,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl AppliedApp {
    pub(super) fn from_result(app: ManagedApp, result: Result<Vec<String>, AppError>) -> Self {
        match result {
            Ok(warnings) => Self {
                app,
                ok: true,
                error: None,
                warnings,
            },
            Err(error) => {
                log::warn!("[xinghuo] 处理 {} 失败: {error}", app.as_str());
                Self {
                    app,
                    ok: false,
                    error: Some(error.to_string()),
                    warnings: Vec::new(),
                }
            }
        }
    }
}

fn api_base(site: &str) -> String {
    format!("{}/v1", site.trim_end_matches('/'))
}

fn claude_model_env(role: &str) -> Option<&'static str> {
    match role {
        "model" | "default" => Some("ANTHROPIC_MODEL"),
        "opus" => Some("ANTHROPIC_DEFAULT_OPUS_MODEL"),
        "sonnet" => Some("ANTHROPIC_DEFAULT_SONNET_MODEL"),
        "haiku" => Some("ANTHROPIC_DEFAULT_HAIKU_MODEL"),
        _ => None,
    }
}

/// Claude Code：基址不带 `/v1`；模型变量只在服务端给了映射时才写。
pub fn claude_settings(site: &str, token: &str, models: &BTreeMap<String, String>) -> Value {
    let mut env = Map::new();
    env.insert(
        "ANTHROPIC_BASE_URL".into(),
        Value::String(site.trim_end_matches('/').to_string()),
    );
    env.insert(
        "ANTHROPIC_AUTH_TOKEN".into(),
        Value::String(token.to_string()),
    );
    for (role, model) in models {
        if let Some(var) = claude_model_env(role) {
            env.insert(var.into(), Value::String(model.clone()));
        }
    }
    json!({ "env": env })
}

/// 与前端 `generateThirdPartyConfig("xinghuo", …)` 生成的 TOML 保持一致。
pub fn codex_config_toml(site: &str, model: &str) -> String {
    let quote = |value: &str| serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into());
    format!(
        "model_provider = \"custom\"\n\
         model = {model}\n\
         model_reasoning_effort = \"high\"\n\
         disable_response_storage = true\n\
         \n\
         [model_providers.custom]\n\
         name = \"xinghuo\"\n\
         base_url = {base}\n\
         wire_api = \"responses\"\n\
         requires_openai_auth = true",
        model = quote(model),
        base = quote(&api_base(site)),
    )
}

pub fn codex_settings(site: &str, token: &str, model: &str) -> Value {
    json!({
        "auth": { "OPENAI_API_KEY": token },
        "config": codex_config_toml(site, model),
    })
}

pub fn opencode_settings(site: &str, token: &str, models: &[String]) -> Value {
    let mut model_map = Map::new();
    for id in models {
        model_map.insert(id.clone(), json!({ "name": id }));
    }
    if model_map.is_empty() {
        model_map.insert(DEFAULT_MODEL.into(), json!({ "name": DEFAULT_MODEL }));
    }
    json!({
        "npm": "@ai-sdk/openai-compatible",
        "name": PROVIDER_NAME,
        "options": {
            "baseURL": api_base(site),
            "apiKey": token,
            "setCacheKey": true,
        },
        "models": model_map,
    })
}

/// Gemini：基址用站点根（CLI 自己拼 /v1beta）；写盘时 security.auth 由
/// write_gemini_live 按 API Key 模式补写。
pub fn gemini_settings(site: &str, token: &str, model: &str) -> Value {
    json!({
        "env": {
            "GOOGLE_GEMINI_BASE_URL": site.trim_end_matches('/'),
            "GEMINI_API_KEY": token,
            "GEMINI_MODEL": model,
        }
    })
}

/// 与 Grok Build 的 config.toml 结构一致：[models] default + [model."<名>"]。
pub fn grokbuild_config_toml(site: &str, token: &str, model: &str) -> String {
    let quote = |value: &str| serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into());
    format!(
        "[models]\n\
         default = {model}\n\
         \n\
         [model.{model}]\n\
         model = {model}\n\
         base_url = {base}\n\
         name = {name}\n\
         api_key = {key}\n\
         api_backend = \"responses\"\n\
         context_window = 256000\n",
        model = quote(model),
        base = quote(&api_base(site)),
        name = quote(PROVIDER_NAME),
        key = quote(token),
    )
}

pub fn grokbuild_settings(site: &str, token: &str, model: &str) -> Value {
    json!({ "config": grokbuild_config_toml(site, token, model) })
}

/// OpenClaw：叠加写进 models.providers.<id>。
pub fn openclaw_settings(site: &str, token: &str, models: &[String]) -> Value {
    json!({
        "baseUrl": api_base(site),
        "apiKey": token,
        "api": "openai-completions",
        "models": model_entries(models),
    })
}

/// Hermes：叠加写进 custom_providers 列表，写盘侧自动补 name 和单数 model 字段。
pub fn hermes_settings(site: &str, token: &str, models: &[String]) -> Value {
    json!({
        "name": PROVIDER_NAME,
        "base_url": api_base(site),
        "api_key": token,
        "api_mode": "chat_completions",
        "models": model_entries(models),
    })
}

/// MiniMax Code：叠加写进 custom_provider.<id>，models 必须是映射。
pub fn mcode_settings(site: &str, token: &str, models: &[String]) -> Value {
    let mut model_map = Map::new();
    for id in models {
        model_map.insert(id.clone(), json!({ "name": id }));
    }
    if model_map.is_empty() {
        model_map.insert(DEFAULT_MODEL.into(), json!({ "name": DEFAULT_MODEL }));
    }
    json!({
        "name": PROVIDER_NAME,
        "kind": "custom",
        "enabled": true,
        "api": "openai-completions",
        "options": {
            "baseURL": api_base(site),
            "apiKey": token,
        },
        "models": model_map,
    })
}

/// Pi：由 Pi 供应商服务写进 models.json。
pub fn pi_settings(site: &str, token: &str, models: &[String]) -> Value {
    json!({
        "name": PROVIDER_NAME,
        "baseUrl": api_base(site),
        "api": "openai-completions",
        "apiKey": token,
        "models": model_entries(models),
    })
}

fn model_entries(models: &[String]) -> Vec<Value> {
    models
        .iter()
        .map(|id| json!({ "id": id, "name": id }))
        .collect()
}

fn plan_models(plan: &ApplyPlan, app: ManagedApp) -> Vec<String> {
    plan.extra_models
        .get(&app)
        .filter(|models| !models.is_empty())
        .cloned()
        .unwrap_or_else(|| vec![DEFAULT_MODEL.to_string()])
}

fn settings_for(app: ManagedApp, site: &str, token: &str, plan: &ApplyPlan) -> Value {
    match app {
        ManagedApp::Claude => claude_settings(site, token, &plan.claude_models),
        ManagedApp::ClaudeDesktop => claude_settings(
            &api_base(site),
            plan.claude_desktop_token.as_deref().unwrap_or(token),
            &BTreeMap::new(),
        ),
        ManagedApp::Codex => codex_settings(site, token, &plan.codex_model),
        ManagedApp::Opencode => opencode_settings(site, token, &plan.opencode_models),
        ManagedApp::Gemini => {
            let models = plan_models(plan, app);
            gemini_settings(site, token, &models[0])
        }
        ManagedApp::GrokBuild => {
            let models = plan_models(plan, app);
            grokbuild_settings(site, token, &models[0])
        }
        ManagedApp::OpenClaw => openclaw_settings(site, token, &plan_models(plan, app)),
        ManagedApp::Hermes => hermes_settings(site, token, &plan_models(plan, app)),
        ManagedApp::Mcode => mcode_settings(site, token, &plan_models(plan, app)),
        ManagedApp::Pi => pi_settings(site, token, &plan_models(plan, app)),
    }
}

fn build_provider(
    app: ManagedApp,
    settings: Value,
    plan: &ApplyPlan,
    existing: Option<&Provider>,
) -> Provider {
    let mut provider = existing.cloned().unwrap_or_else(|| {
        let mut fresh = Provider::with_id(
            app.provider_id().to_string(),
            PROVIDER_NAME.to_string(),
            Value::Null,
            None,
        );
        fresh.created_at = Some(now_ms());
        fresh
    });
    provider.name = PROVIDER_NAME.to_string();
    provider.settings_config = settings;
    provider.website_url = Some(super::site_url());
    provider.category = Some("third_party".to_string());
    provider.icon = Some(PROVIDER_ICON.to_string());
    // 固定接入不继承旧编辑器的端点、协议和覆盖项，避免把请求改回其他接口。
    provider.meta = Some(ProviderMeta {
        common_config_enabled: Some(true),
        ..ProviderMeta::default()
    });
    if app == ManagedApp::ClaudeDesktop {
        let meta = provider.meta.as_mut().expect("元数据已初始化");
        meta.common_config_enabled = Some(false);
        meta.claude_desktop_mode = Some(ClaudeDesktopMode::Proxy);
        let models = desktop_models(plan);
        meta.api_format = Some(plan.claude_desktop_api_format.clone());
        // 保留接口真实模型 ID；仅明确下发的其他品牌模型需要桌面兼容路由。
        meta.claude_desktop_model_routes = models
            .into_iter()
            .map(|model| {
                (
                    model.clone(),
                    ClaudeDesktopModelRoute {
                        label_override: Some(model.clone()),
                        model,
                        supports_1m: Some(false),
                    },
                )
            })
            .collect();
    }
    set_usage_script(&mut provider);
    provider
}

fn desktop_models(plan: &ApplyPlan) -> Vec<String> {
    plan.claude_desktop_model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(|model| vec![model.to_string()])
        .unwrap_or_else(|| plan.claude_desktop_models.clone())
}

fn desktop_model_error(error: &XinghuoError) -> String {
    match error.code.as_str() {
        "API_KEY_QUOTA_EXHAUSTED" => {
            "接口密钥额度已用完，未修改桌面配置；请充值或更换有额度的密钥".into()
        }
        "KEY_INVALID" => "接口密钥无效，未修改桌面配置".into(),
        _ if matches!(error.status, Some(401 | 403)) => {
            "接口密钥无效或没有模型权限，未修改桌面配置".into()
        }
        "NETWORK" => "读取模型列表时网络连接失败，未修改桌面配置；请稍后重试".into(),
        _ => format!(
            "读取模型列表失败（{}{}），未修改桌面配置；请稍后重试",
            error.code,
            error
                .status
                .map(|status| format!("，HTTP {status}"))
                .unwrap_or_default()
        ),
    }
}

/// 卡片余额走原生的星火用量模板，5 分钟自动刷新一次。
fn set_usage_script(provider: &mut Provider) {
    provider
        .meta
        .get_or_insert_with(ProviderMeta::default)
        .usage_script = Some(super::usage_script());
}

fn has_usage_script(provider: &Provider) -> bool {
    super::is_usage_script(
        provider
            .meta
            .as_ref()
            .and_then(|meta| meta.usage_script.as_ref()),
    )
}

fn apply_one(
    state: &AppState,
    app: ManagedApp,
    site: &str,
    token: &str,
    plan: &ApplyPlan,
    renewal: bool,
    record: &mut ActivationRecord,
) -> Result<Vec<String>, AppError> {
    let app_type = app.app_type();
    let id = app.provider_id();
    let providers = ProviderService::list(state, app_type.clone())?;
    let existing = providers.get(id);

    // 续费拿回的是同一把密钥：配置保持不动，也不抢回顾客手动切走的供应商。
    // 老版本建的供应商没有余额查询配置，只补这一项元数据。
    if renewal {
        if let Some(existing) = existing {
            if app == ManagedApp::ClaudeDesktop
                && ProviderService::current(state, app_type.clone())? == id
            {
                tauri::async_runtime::block_on(state.proxy_service.start())
                    .map_err(AppError::Message)?;
            }
            if !has_usage_script(existing) {
                let mut provider = existing.clone();
                set_usage_script(&mut provider);
                ProviderService::update(state, app_type, None, provider)?;
            }
            return Ok(Vec::new());
        }
    }

    if app == ManagedApp::ClaudeDesktop {
        if let Some(error) = &plan.claude_desktop_model_error {
            return Err(AppError::Message(desktop_model_error(error)));
        }
    }
    if app == ManagedApp::ClaudeDesktop && desktop_models(plan).is_empty() {
        return Err(AppError::Message("接口没有返回可用的 Claude Desktop 模型，未修改桌面配置；请检查密钥的模型权限或刷新后重试".into()));
    }
    super::restore::capture(state, &app_type)?;

    // 记下激活前的当前供应商，只记第一次；叠加类应用没有「当前」的概念。
    if !app_type.is_additive_mode() && !record.previous.contains_key(app.as_str()) {
        let current = ProviderService::current(state, app_type.clone())?;
        if !current.is_empty() && current != id {
            record.previous.insert(app.as_str().to_string(), current);
        }
    }

    // 自动保留偏好配置，同时剥离旧片段中的模型路由和凭据。
    // MCP 仍由已有的独立同步流程维护，顾客无需操作共享配置。
    if let Some(snippet) = state.db.get_config_snippet(app.as_str())? {
        let settings = match app {
            ManagedApp::Codex => json!({ "config": snippet }),
            ManagedApp::Claude => serde_json::from_str(&snippet)
                .map_err(|error| AppError::Message(format!("读取偏好配置失败: {error}")))?,
            _ => Value::Null,
        };
        if matches!(app, ManagedApp::Claude | ManagedApp::Codex) {
            let safe = ProviderService::extract_common_config_snippet_from_settings(
                app_type.clone(),
                &settings,
            )?;
            state.db.set_config_snippet(app.as_str(), Some(safe))?;
        }
    }

    let provider = build_provider(app, settings_for(app, site, token, plan), plan, existing);
    if app == ManagedApp::ClaudeDesktop {
        tauri::async_runtime::block_on(state.proxy_service.start()).map_err(AppError::Message)?;
    }
    if existing.is_some() {
        ProviderService::update(state, app_type.clone(), None, provider)?;
    } else {
        ProviderService::add(state, app_type.clone(), provider, true)?;
    }
    Ok(ProviderService::switch(state, app_type, id)?.warnings)
}

/// 为计划里的每个工具建立或更新星火供应商并切换过去。单个工具失败不影响其他工具。
pub fn apply_providers(
    state: &AppState,
    token: &str,
    plan: &ApplyPlan,
    renewal: bool,
    record: &mut ActivationRecord,
) -> Vec<AppliedApp> {
    let site = super::site_url();
    plan.apps
        .iter()
        .map(|&app| {
            AppliedApp::from_result(
                app,
                apply_one(state, app, &site, token, plan, renewal, record),
            )
        })
        .collect()
}

/// 仅清除匹配激活密钥的认证，保留用户其他配置。
fn clear_managed_auth(app: ManagedApp, token: &str) -> Result<(), AppError> {
    match app {
        ManagedApp::Claude => {
            let path = crate::config::get_claude_settings_path();
            if !path.exists() {
                return Ok(());
            }
            let mut settings: Value = crate::config::read_json_file(&path)?;
            if let Some(env) = settings.get_mut("env").and_then(Value::as_object_mut) {
                if env.get("ANTHROPIC_AUTH_TOKEN").and_then(Value::as_str) == Some(token) {
                    env.remove("ANTHROPIC_AUTH_TOKEN");
                }
            }
            crate::config::write_json_file(&path, &settings)?;
        }
        ManagedApp::Codex => {
            let live = crate::codex_config::read_codex_live_settings()?;
            let config = live
                .get("config")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let cleaned =
                crate::codex_config::remove_codex_experimental_bearer_token_if(config, |key| {
                    key == token
                })?;
            let mut doc = cleaned
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| AppError::Message(format!("Codex 配置无效: {e}")))?;
            // 失活后禁止用官方登录凭据替代星火密钥。
            if let Some(table) = doc
                .get_mut("model_providers")
                .and_then(|v| v.get_mut("custom"))
            {
                table["requires_openai_auth"] = toml_edit::value(false);
            }
            let mut auth = live.get("auth").cloned().unwrap_or_else(|| json!({}));
            if auth.get("OPENAI_API_KEY").and_then(Value::as_str) == Some(token) {
                if let Some(auth) = auth.as_object_mut() {
                    auth.remove("OPENAI_API_KEY");
                }
            }
            crate::codex_config::write_codex_live_atomic(&auth, Some(&doc.to_string()))?;
        }
        ManagedApp::Gemini => {
            let path = crate::gemini_config::get_gemini_env_path();
            if !path.exists() {
                return Ok(());
            }
            let content = std::fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?;
            let mut env = crate::gemini_config::parse_env_file(&content);
            if env.get("GEMINI_API_KEY").map(String::as_str) == Some(token) {
                env.remove("GEMINI_API_KEY");
                crate::gemini_config::write_gemini_env_atomic(&env)?;
            }
        }
        ManagedApp::GrokBuild => {
            let path = crate::grok_config::get_grok_config_path();
            if !path.exists() {
                return Ok(());
            }
            let content = std::fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?;
            let mut doc = content
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| AppError::Message(format!("Grok Build 配置无效: {e}")))?;
            let mut changed = false;
            if let Some(models) = doc.get_mut("model").and_then(|v| v.as_table_like_mut()) {
                for (_, entry) in models.iter_mut() {
                    let Some(table) = entry.as_table_like_mut() else {
                        continue;
                    };
                    if table.get("api_key").and_then(|v| v.as_str()) == Some(token) {
                        table.remove("api_key");
                        changed = true;
                    }
                }
            }
            if changed {
                crate::grok_config::write_grok_live_settings(&json!({
                    "config": doc.to_string()
                }))?;
            }
        }
        // 叠加类应用没有「当前」的概念，删除供应商时 live 配置一并移除。
        ManagedApp::ClaudeDesktop
        | ManagedApp::Opencode
        | ManagedApp::OpenClaw
        | ManagedApp::Hermes
        | ManagedApp::Mcode
        | ManagedApp::Pi => {}
    }
    Ok(())
}

fn deactivate_one(
    state: &AppState,
    app: ManagedApp,
    record: &ActivationRecord,
) -> Result<Vec<String>, AppError> {
    let app_type = app.app_type();
    let id = app.provider_id();
    let providers = ProviderService::list(state, app_type.clone())?;
    if !providers.contains_key(id) {
        return Ok(Vec::new());
    }

    let mut warnings = Vec::new();
    if !app_type.is_additive_mode() && ProviderService::current(state, app_type.clone())? == id {
        if app == ManagedApp::ClaudeDesktop {
            super::restore::restore_app(state, app)?;
            ProviderService::delete(state, app_type, id)?;
            return Ok(warnings);
        }
        // 有可恢复的供应商时切回；否则只移除星火认证，不创建官方配置。
        if let Some(previous) = record
            .previous
            .get(app.as_str())
            .filter(|prev| prev.as_str() != id && providers.contains_key(prev.as_str()))
        {
            warnings.extend(ProviderService::switch(state, app_type.clone(), previous)?.warnings);
        } else {
            let provider = providers.get(id).expect("供应商已确认存在");
            let token = match app {
                ManagedApp::Claude => provider
                    .settings_config
                    .get("env")
                    .and_then(|env| env.get("ANTHROPIC_AUTH_TOKEN")),
                ManagedApp::Codex => provider
                    .settings_config
                    .get("auth")
                    .and_then(|auth| auth.get("OPENAI_API_KEY")),
                ManagedApp::Gemini => provider
                    .settings_config
                    .get("env")
                    .and_then(|env| env.get("GEMINI_API_KEY")),
                _ => None,
            }
            .and_then(Value::as_str)
            .unwrap_or(&record.token);
            clear_managed_auth(app, token)?;
            crate::settings::set_current_provider(&app_type, None)?;
            state.db.set_current_provider(app_type.as_str(), "")?;
        }
    }
    ProviderService::delete(state, app_type, id)?;
    Ok(warnings)
}

/// 删除所有星火供应商（不论记录里配过哪些），单选应用先切回激活前的供应商。
pub fn deactivate_providers(state: &AppState, record: &ActivationRecord) -> Vec<AppliedApp> {
    ManagedApp::ALL
        .into_iter()
        .map(|app| AppliedApp::from_result(app, deactivate_one(state, app, record)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_uses_site_root_and_optional_model_env() {
        let settings = claude_settings("https://ai.wfpromax.xyz/", "sk-1", &BTreeMap::new());
        assert_eq!(
            settings["env"]["ANTHROPIC_BASE_URL"],
            "https://ai.wfpromax.xyz"
        );
        assert_eq!(settings["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-1");
        assert_eq!(settings["env"].as_object().unwrap().len(), 2);

        let models = BTreeMap::from([
            ("opus".to_string(), "gpt-5.5".to_string()),
            ("unknown".to_string(), "x".to_string()),
        ]);
        let with_models = claude_settings("https://ai.wfpromax.xyz", "sk-1", &models);
        assert_eq!(
            with_models["env"]["ANTHROPIC_DEFAULT_OPUS_MODEL"],
            "gpt-5.5"
        );
        assert_eq!(with_models["env"].as_object().unwrap().len(), 3);
    }

    #[test]
    fn codex_toml_matches_frontend_template() {
        let toml = codex_config_toml("https://ai.wfpromax.xyz", "gpt-5.5");
        assert_eq!(
            toml,
            "model_provider = \"custom\"\nmodel = \"gpt-5.5\"\nmodel_reasoning_effort = \"high\"\ndisable_response_storage = true\n\n[model_providers.custom]\nname = \"xinghuo\"\nbase_url = \"https://ai.wfpromax.xyz/v1\"\nwire_api = \"responses\"\nrequires_openai_auth = true"
        );
        let settings = codex_settings("https://ai.wfpromax.xyz", "sk-2", "gpt-5.5");
        assert_eq!(settings["auth"]["OPENAI_API_KEY"], "sk-2");
    }

    #[test]
    fn opencode_uses_openai_compatible_with_v1_base() {
        let settings = opencode_settings(
            "https://ai.wfpromax.xyz",
            "sk-3",
            &["gpt-5.5".to_string(), "gpt-5.5-mini".to_string()],
        );
        assert_eq!(settings["npm"], "@ai-sdk/openai-compatible");
        assert_eq!(settings["options"]["baseURL"], "https://ai.wfpromax.xyz/v1");
        assert_eq!(settings["options"]["apiKey"], "sk-3");
        assert_eq!(settings["models"].as_object().unwrap().len(), 2);

        let fallback = opencode_settings("https://ai.wfpromax.xyz", "sk-3", &[]);
        assert!(fallback["models"].get(DEFAULT_MODEL).is_some());
    }

    #[test]
    fn gemini_uses_site_root_and_api_key_env() {
        let settings = gemini_settings("https://ai.wfpromax.xyz/", "sk-4", "gpt-5.5");
        assert_eq!(
            settings["env"]["GOOGLE_GEMINI_BASE_URL"],
            "https://ai.wfpromax.xyz"
        );
        assert_eq!(settings["env"]["GEMINI_API_KEY"], "sk-4");
        assert_eq!(settings["env"]["GEMINI_MODEL"], "gpt-5.5");
    }

    #[test]
    fn grokbuild_toml_passes_native_validation() {
        let toml = grokbuild_config_toml("https://ai.wfpromax.xyz", "sk-5", "gpt-5.5");
        crate::grok_config::validate_config_toml(&toml).expect("valid Grok Build config");
        assert!(toml.contains("default = \"gpt-5.5\""));
        assert!(toml.contains("[model.\"gpt-5.5\"]"));
        assert!(toml.contains("base_url = \"https://ai.wfpromax.xyz/v1\""));
        assert!(toml.contains("api_key = \"sk-5\""));
        let settings = grokbuild_settings("https://ai.wfpromax.xyz", "sk-5", "gpt-5.5");
        assert_eq!(settings["config"].as_str(), Some(toml.as_str()));
    }

    #[test]
    fn additive_platforms_use_openai_compatible_v1_base() {
        let models = vec!["gpt-5.5".to_string(), "gpt-5.5-mini".to_string()];

        let openclaw = openclaw_settings("https://ai.wfpromax.xyz", "sk-6", &models);
        assert_eq!(openclaw["baseUrl"], "https://ai.wfpromax.xyz/v1");
        assert_eq!(openclaw["apiKey"], "sk-6");
        assert_eq!(openclaw["api"], "openai-completions");
        assert_eq!(openclaw["models"][0]["id"], "gpt-5.5");
        let typed: crate::openclaw_config::OpenClawProviderConfig =
            serde_json::from_value(openclaw).expect("OpenClaw typed config");
        assert_eq!(typed.models.len(), 2);

        let hermes = hermes_settings("https://ai.wfpromax.xyz", "sk-6", &models);
        assert_eq!(hermes["name"], PROVIDER_NAME);
        assert_eq!(hermes["base_url"], "https://ai.wfpromax.xyz/v1");
        assert_eq!(hermes["api_key"], "sk-6");
        assert_eq!(hermes["api_mode"], "chat_completions");
        assert_eq!(hermes["models"][1]["id"], "gpt-5.5-mini");

        let mcode = mcode_settings("https://ai.wfpromax.xyz", "sk-6", &models);
        assert_eq!(mcode["kind"], "custom");
        assert_eq!(mcode["enabled"], true);
        assert_eq!(mcode["api"], "openai-completions");
        assert_eq!(mcode["options"]["baseURL"], "https://ai.wfpromax.xyz/v1");
        assert_eq!(mcode["options"]["apiKey"], "sk-6");
        assert_eq!(mcode["models"].as_object().unwrap().len(), 2);
        crate::mcode_config::validate_provider("xinghuo-mcode", &mcode)
            .expect("valid MCode provider");

        let pi = pi_settings("https://ai.wfpromax.xyz", "sk-6", &models);
        assert_eq!(pi["name"], PROVIDER_NAME);
        assert_eq!(pi["baseUrl"], "https://ai.wfpromax.xyz/v1");
        assert_eq!(pi["apiKey"], "sk-6");
        assert_eq!(pi["models"][0]["id"], "gpt-5.5");
    }

    #[test]
    fn extra_platforms_fall_back_to_default_model() {
        let plan = ApplyPlan::defaults();
        for app in ManagedApp::EXTRA {
            let settings = settings_for(app, "https://ai.wfpromax.xyz", "sk-7", &plan);
            assert!(
                settings.to_string().contains(DEFAULT_MODEL),
                "{} should fall back to the default model",
                app.as_str()
            );
        }

        let mut plan = ApplyPlan::defaults();
        plan.extra_models.insert(
            ManagedApp::Gemini,
            vec!["gemini-pro".to_string(), "gemini-flash".to_string()],
        );
        plan.extra_models
            .insert(ManagedApp::Pi, vec!["pi-model".to_string()]);
        let gemini = settings_for(
            ManagedApp::Gemini,
            "https://ai.wfpromax.xyz",
            "sk-7",
            &plan,
        );
        assert_eq!(gemini["env"]["GEMINI_MODEL"], "gemini-pro");
        let pi = settings_for(ManagedApp::Pi, "https://ai.wfpromax.xyz", "sk-7", &plan);
        assert_eq!(pi["models"][0]["id"], "pi-model");
    }
}
