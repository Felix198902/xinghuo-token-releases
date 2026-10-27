//! 激活相关的本机文件：激活记录、凭据钱包。
//!
//! 都放在用户主目录下的应用数据目录里，不跟随「自定义配置目录」。
//! 含密钥的文件以 0600 原子写入；凭据本身不绑定设备。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::{now_ms, ManagedApp};
use crate::error::AppError;

const ACTIVATION_FILE: &str = "activation.json";
const WALLET_FILE: &str = "wallet.json";
const WALLET_HISTORY_LIMIT: usize = 5;

fn local_dir() -> PathBuf {
    crate::config::get_home_dir().join(crate::config::APP_CONFIG_DIR_NAME)
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Quota {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default)]
    pub total: f64,
    #[serde(default)]
    pub used: f64,
    #[serde(default)]
    pub remaining: f64,
}

/// 本机激活记录（activation.json）。`token` 是真正的凭据。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivationRecord {
    pub token: String,
    /// 桌面专用凭据与主套餐独立，仅保存在本机私有文件中。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desktop_connection: Option<DesktopConnection>,
    /// `cdk`（激活码换来的）或 `key`（顾客直接粘贴的 sk- 密钥）。
    #[serde(default)]
    pub source: String,
    /// 原始输入的 SHA-256，用于相同 CDK 的本机幂等，不保存 CDK 明文。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_hash: Option<String>,
    #[serde(default)]
    pub activated_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota: Option<Quota>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance_scope: Option<String>,
    /// 最近一次查询到的密钥有效性；None 表示还没查过。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid: Option<bool>,
    /// 已由激活页配置好的工具。
    #[serde(default)]
    pub apps: Vec<ManagedApp>,
    /// 激活前各工具的当前供应商（应用名 → 供应商 ID），退出激活时切回去。
    #[serde(default)]
    pub previous: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shop_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopConnection {
    pub token: String,
    pub api_format: String,
    #[serde(default)]
    pub model: Option<String>,
}

/// 凭据钱包（wallet.json）：退出激活后仍保留，用作下次续费的持有证明。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wallet {
    pub token: String,
    #[serde(default)]
    pub saved_at: i64,
    /// 换过密钥时保留的旧密钥，新的在前。
    #[serde(default)]
    pub history: Vec<String>,
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(value) => Some(value),
        Err(e) => {
            log::warn!("[xinghuo] 解析 {} 失败: {e}", path.display());
            None
        }
    }
}

fn write_private_json<T: Serialize>(path: &Path, value: &T) -> Result<(), AppError> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|e| AppError::JsonSerialize { source: e })?;
    crate::config::atomic_write_private(path, &bytes)
}

pub fn load_activation() -> Option<ActivationRecord> {
    read_json(&local_dir().join(ACTIVATION_FILE))
}

pub fn save_activation(record: &ActivationRecord) -> Result<(), AppError> {
    write_private_json(&local_dir().join(ACTIVATION_FILE), record)
}

pub fn clear_activation() -> Result<(), AppError> {
    let path = local_dir().join(ACTIVATION_FILE);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AppError::io(&path, e)),
    }
}

pub fn load_wallet() -> Option<Wallet> {
    read_json(&local_dir().join(WALLET_FILE))
}

/// 把新密钥记进钱包；原来的密钥挪进历史（最多保留 5 把）。
pub fn save_wallet_token(token: &str) -> Result<(), AppError> {
    save_wallet_token_in(&local_dir(), token)
}

fn save_wallet_token_in(dir: &Path, token: &str) -> Result<(), AppError> {
    let path = dir.join(WALLET_FILE);
    let mut wallet: Wallet = read_json(&path).unwrap_or_default();
    if wallet.token != token {
        let previous = std::mem::replace(&mut wallet.token, token.to_string());
        wallet.history.retain(|t| t != token && t != &previous);
        if !previous.is_empty() {
            wallet.history.insert(0, previous);
        }
        wallet.history.truncate(WALLET_HISTORY_LIMIT);
    }
    wallet.saved_at = now_ms();
    write_private_json(&path, &wallet)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallet_keeps_latest_token_and_bounded_history() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..8 {
            save_wallet_token_in(dir.path(), &format!("sk-{i}")).unwrap();
        }
        save_wallet_token_in(dir.path(), "sk-7").unwrap();
        let wallet: Wallet = read_json(&dir.path().join(WALLET_FILE)).unwrap();
        assert_eq!(wallet.token, "sk-7");
        assert_eq!(wallet.history, vec!["sk-6", "sk-5", "sk-4", "sk-3", "sk-2"]);

        // 换回历史里的旧密钥：从历史中移出，不会重复
        save_wallet_token_in(dir.path(), "sk-4").unwrap();
        let wallet: Wallet = read_json(&dir.path().join(WALLET_FILE)).unwrap();
        assert_eq!(wallet.token, "sk-4");
        assert_eq!(wallet.history, vec!["sk-7", "sk-6", "sk-5", "sk-3", "sk-2"]);
    }

    #[cfg(unix)]
    #[test]
    fn wallet_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        save_wallet_token_in(dir.path(), "sk-secret").unwrap();
        let mode = std::fs::metadata(dir.path().join(WALLET_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn activation_record_tolerates_missing_optional_fields() {
        let record: ActivationRecord = serde_json::from_str(r#"{"token":"sk-1"}"#).unwrap();
        assert_eq!(record.token, "sk-1");
        assert!(record.apps.is_empty());
        assert!(record.previous.is_empty());
    }
}
