//! 激活相关的本机文件：设备号、激活记录、凭据钱包。
//!
//! 都放在用户主目录下的应用数据目录里，不跟随「自定义配置目录」——那个目录
//! 可能是云同步盘，多台电脑会因此共用同一个设备号和密钥。含密钥的文件以 0600
//! 原子写入。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::{now_ms, ManagedApp};
use crate::error::AppError;

const DEVICE_ID_FILE: &str = "device_id";
const ACTIVATION_FILE: &str = "activation.json";
const WALLET_FILE: &str = "wallet.json";
const WALLET_HISTORY_LIMIT: usize = 5;

/// 服务端拒收短于 16 位的低熵设备号（DEVICE_ID_INVALID）。
const DEVICE_ID_MIN_LEN: usize = 16;
const DEVICE_ID_MAX_LEN: usize = 128;

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

fn is_valid_device_id(value: &str) -> bool {
    (DEVICE_ID_MIN_LEN..=DEVICE_ID_MAX_LEN).contains(&value.len())
}

/// 本机设备号：首次生成随机 UUID 并落盘，跨重装保留。
///
/// 盘写不进去时也不退回常量——所有写不了盘的机器共用同一个设备号，服务端
/// 就会把彼此的续费混到一起。生成的值记在进程内，同一次运行保持稳定。
pub fn device_id() -> String {
    device_id_in(&local_dir())
}

fn device_id_in(dir: &Path) -> String {
    static GENERATED: Mutex<Option<String>> = Mutex::new(None);

    let path = dir.join(DEVICE_ID_FILE);
    if let Ok(saved) = std::fs::read_to_string(&path) {
        let saved = saved.trim();
        if is_valid_device_id(saved) {
            return saved.to_string();
        }
    }

    let mut generated = GENERATED.lock().unwrap_or_else(|p| p.into_inner());
    let id = generated
        .get_or_insert_with(|| uuid::Uuid::new_v4().to_string())
        .clone();
    if let Err(e) = crate::config::atomic_write_private(&path, id.as_bytes()) {
        log::warn!("[xinghuo] 写入设备号失败，本次运行沿用内存中的值: {e}");
    }
    id
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
    fn device_id_length_rules_match_server() {
        assert!(!is_valid_device_id("dev-fallback"));
        assert!(is_valid_device_id(&uuid::Uuid::new_v4().to_string()));
        assert!(!is_valid_device_id(&"x".repeat(129)));
    }

    #[test]
    fn device_id_is_generated_once_and_reused() {
        let dir = tempfile::tempdir().unwrap();
        let first = device_id_in(dir.path());
        assert!(is_valid_device_id(&first));
        assert_eq!(device_id_in(dir.path()), first);
        let saved = std::fs::read_to_string(dir.path().join(DEVICE_ID_FILE)).unwrap();
        assert_eq!(saved, first);
    }

    #[test]
    fn low_entropy_device_id_on_disk_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(DEVICE_ID_FILE), "dev-fallback").unwrap();
        let id = device_id_in(dir.path());
        assert_ne!(id, "dev-fallback");
        assert!(is_valid_device_id(&id));
    }

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
