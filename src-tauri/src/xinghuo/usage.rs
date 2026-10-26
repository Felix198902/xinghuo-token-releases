//! 星火供应商的余额查询：供应商卡片和托盘用的原生用量模板。
//!
//! 不用通用 JS 脚本：它在非 2xx 时会把「HTTP 401 : <服务端原文>」直接显示在
//! 卡片上，违反「顾客只看到现象、错误码和联系方式」的规矩。这里自己查
//! /api/v1/plugin/status，把各种情况翻成简短的状态文案。

use crate::provider::{UsageData, UsageResult, UsageScript};

use super::{api, site_url, storage, PROVIDER_NAME};

/// 用量模板类型，写在供应商 meta.usage_script.templateType 里。
pub const USAGE_TEMPLATE_TYPE: &str = "xinghuo";
/// 自动查询间隔（分钟）。
const AUTO_QUERY_INTERVAL_MINUTES: u64 = 5;
/// 服务端未下发时的余额单位。
const DEFAULT_UNIT_LABEL: &str = "¥";

/// 写进星火供应商的用量配置：走原生模板，不需要脚本代码。
pub fn usage_script() -> UsageScript {
    UsageScript {
        enabled: true,
        language: "javascript".to_string(),
        code: String::new(),
        timeout: Some(20),
        api_key: None,
        base_url: None,
        access_token: None,
        user_id: None,
        template_type: Some(USAGE_TEMPLATE_TYPE.to_string()),
        auto_query_interval: Some(AUTO_QUERY_INTERVAL_MINUTES),
        coding_plan_provider: None,
        access_key_id: None,
        secret_access_key: None,
        team_organization_id: None,
        team_project_id: None,
    }
}

pub fn is_usage_script(script: Option<&UsageScript>) -> bool {
    script.and_then(|s| s.template_type.as_deref()) == Some(USAGE_TEMPLATE_TYPE)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lang {
    Zh,
    En,
}

fn current_lang() -> Lang {
    match crate::settings::get_settings().language.as_deref() {
        Some("en") => Lang::En,
        _ => Lang::Zh,
    }
}

fn text(lang: Lang, zh: &str, en: &str) -> String {
    match lang {
        Lang::Zh => zh.to_string(),
        Lang::En => en.to_string(),
    }
}

fn expiry_text(lang: Lang, expires_at: Option<&str>) -> String {
    let Some(raw) = expires_at.filter(|v| !v.is_empty()) else {
        return text(lang, "长期有效", "No expiry");
    };
    let date = chrono::DateTime::parse_from_rfc3339(raw)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_else(|_| raw.to_string());
    match lang {
        Lang::Zh => format!("到期 {date}"),
        Lang::En => format!("Expires {date}"),
    }
}

fn plan_data(plan: Option<String>) -> UsageData {
    UsageData {
        plan_name: Some(
            plan.filter(|p| !p.trim().is_empty())
                .unwrap_or_else(|| PROVIDER_NAME.to_string()),
        ),
        extra: None,
        is_valid: None,
        invalid_message: None,
        total: None,
        used: None,
        remaining: None,
        unit: None,
    }
}

fn result_for_status(lang: Lang, unit_label: &str, status: api::StatusReply) -> UsageResult {
    let quota = status.quota.unwrap_or_default();
    let valid = status.valid != Some(false);
    let mut data = plan_data(status.plan);
    data.extra = Some(expiry_text(lang, status.expires_at.as_deref()));
    data.is_valid = Some(valid);
    if !valid {
        data.invalid_message = Some(if quota.remaining <= 0.0 {
            if status.balance_scope.as_deref() == Some("key_cap")
                && quota.total > 0.0
                && quota.used >= quota.total
            {
                text(lang, "密钥额度已用完", "Key quota used up")
            } else {
                text(lang, "余额已用完", "Balance used up")
            }
        } else {
            text(lang, "已过期或停用", "Expired or disabled")
        });
    }
    data.total = Some(quota.total);
    data.used = Some(quota.used);
    data.remaining = Some(quota.remaining);
    data.unit = Some(unit_label.to_string());
    UsageResult {
        success: true,
        data: Some(vec![data]),
        error: None,
    }
}

fn result_for_invalid_key(lang: Lang) -> UsageResult {
    let mut data = plan_data(None);
    data.is_valid = Some(false);
    data.invalid_message = Some(text(lang, "密钥无效", "Invalid key"));
    UsageResult {
        success: true,
        data: Some(vec![data]),
        error: None,
    }
}

fn result_for_server_error(lang: Lang, status: Option<u16>, contact: &str) -> UsageResult {
    let code = match status {
        Some(s @ 100..=999) => format!("XH-4{s}"),
        _ => "XH-4000".to_string(),
    };
    let message = match lang {
        Lang::Zh => format!("服务暂时无法响应（错误码 {code}）· 请把错误码发给系统管理员 {contact}"),
        Lang::En => format!(
            "Service temporarily unavailable (error code {code}) · Send the error code to the administrator: {contact}"
        ),
    };
    UsageResult {
        success: false,
        data: None,
        error: Some(message),
    }
}

/// 查一把密钥的余额。
///
/// - 成功：套餐名、到期、总额 / 已用 / 剩余，失效时带简短原因；
/// - 401 / 403：标记为「密钥无效」；
/// - 网络不通或限流：返回 Err，前端保留上一次成功的数据；
/// - 其他服务端错误：只显示 XH-4<状态码> 和联系方式。
pub async fn query_usage(token: &str) -> Result<UsageResult, String> {
    let lang = current_lang();
    let record = storage::load_activation();
    let unit_label = record
        .as_ref()
        .and_then(|r| r.unit_label.clone())
        .unwrap_or_else(|| DEFAULT_UNIT_LABEL.to_string());
    let contact = record
        .as_ref()
        .and_then(|r| r.contact.clone())
        .unwrap_or_else(|| super::DEFAULT_CONTACT.to_string());

    match api::status(&site_url(), token).await {
        Ok(status) => Ok(result_for_status(lang, &unit_label, status)),
        Err(error) if error.code == "KEY_INVALID" => Ok(result_for_invalid_key(lang)),
        Err(error) if error.code == "NETWORK" || error.code == "RATE_LIMITED" => {
            Err(error.to_string())
        }
        Err(error) => Ok(result_for_server_error(lang, error.status, &contact)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xinghuo::Quota;

    fn status(valid: Option<bool>, remaining: f64, expires_at: Option<&str>) -> api::StatusReply {
        api::StatusReply {
            ok: true,
            valid,
            balance_scope: None,
            plan: Some("星火客户端".to_string()),
            expires_at: expires_at.map(str::to_string),
            quota: Some(Quota {
                unit: Some("USD".to_string()),
                total: 100.0,
                used: 100.0 - remaining,
                remaining,
            }),
        }
    }

    #[test]
    fn usage_script_uses_native_template_and_five_minute_refresh() {
        let script = usage_script();
        assert!(script.enabled);
        assert!(is_usage_script(Some(&script)));
        assert_eq!(script.auto_query_interval, Some(5));
        assert!(script.code.is_empty());
        assert!(!is_usage_script(None));
    }

    #[test]
    fn valid_status_shows_balance_with_unit_label() {
        let result = result_for_status(Lang::Zh, "¥", status(Some(true), 88.5, None));
        assert!(result.success);
        let data = &result.data.unwrap()[0];
        assert_eq!(data.plan_name.as_deref(), Some("星火客户端"));
        assert_eq!(data.remaining, Some(88.5));
        assert_eq!(data.unit.as_deref(), Some("¥"));
        assert_eq!(data.is_valid, Some(true));
        assert!(data.invalid_message.is_none());
        assert_eq!(data.extra.as_deref(), Some("长期有效"));
    }

    #[test]
    fn exhausted_and_expired_keys_get_short_reasons() {
        let exhausted = result_for_status(Lang::Zh, "¥", status(Some(false), 0.0, None));
        let data = &exhausted.data.unwrap()[0];
        assert_eq!(data.is_valid, Some(false));
        assert_eq!(data.invalid_message.as_deref(), Some("余额已用完"));

        let expired = result_for_status(
            Lang::En,
            "¥",
            status(Some(false), 12.0, Some("2026-01-02T00:00:00Z")),
        );
        let data = &expired.data.unwrap()[0];
        assert_eq!(data.invalid_message.as_deref(), Some("Expired or disabled"));
        assert!(data
            .extra
            .as_deref()
            .unwrap()
            .starts_with("Expires 2026-01-0"));
    }

    #[test]
    fn key_cap_exhaustion_differs_from_account_balance_exhaustion() {
        let mut reply = status(Some(false), 0.0, None);
        reply.balance_scope = Some("key_cap".to_string());
        let data = result_for_status(Lang::Zh, "¥", reply.clone())
            .data
            .unwrap();
        assert_eq!(data[0].invalid_message.as_deref(), Some("密钥额度已用完"));

        reply.quota.as_mut().unwrap().used = 20.0;
        let data = result_for_status(Lang::Zh, "¥", reply).data.unwrap();
        assert_eq!(data[0].invalid_message.as_deref(), Some("余额已用完"));
    }

    #[test]
    fn invalid_key_and_server_errors_never_show_raw_text() {
        let invalid = result_for_invalid_key(Lang::Zh);
        assert_eq!(
            invalid.data.unwrap()[0].invalid_message.as_deref(),
            Some("密钥无效")
        );

        let server = result_for_server_error(Lang::Zh, Some(502), "QQ 1");
        assert!(!server.success);
        let message = server.error.unwrap();
        assert!(message.contains("XH-4502"));
        assert!(message.contains("QQ 1"));
    }
}
