//! 切换前的本机配置快照；不创建 default 或官方供应商。
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use super::{AppliedApp, ManagedApp};
use crate::{app_config::AppType, error::AppError, store::AppState};
use serde::{Deserialize, Serialize};

static SNAPSHOT_LOCK: Mutex<()> = Mutex::new(());

#[derive(Default, Serialize, Deserialize)]
struct Snapshot {
    files: BTreeMap<PathBuf, Option<Vec<u8>>>,
    current: Option<String>,
    #[serde(default)]
    restored: bool,
}

fn path() -> PathBuf {
    crate::config::get_home_dir()
        .join(crate::config::APP_CONFIG_DIR_NAME)
        .join("restore.json")
}

fn load() -> Result<BTreeMap<String, Snapshot>, AppError> {
    let path = path();
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    crate::config::read_json_file(&path)
}

fn save(snapshots: &BTreeMap<String, Snapshot>) -> Result<(), AppError> {
    let bytes =
        serde_json::to_vec(snapshots).map_err(|source| AppError::JsonSerialize { source })?;
    crate::config::atomic_write_private(&path(), &bytes)
}

fn files(app: ManagedApp) -> Result<Vec<PathBuf>, AppError> {
    Ok(match app {
        ManagedApp::Claude => vec![crate::config::get_claude_settings_path()],
        ManagedApp::ClaudeDesktop => crate::claude_desktop_config::managed_config_paths()?,
        ManagedApp::Codex => vec![
            crate::codex_config::get_codex_config_path(),
            crate::codex_config::get_codex_auth_path(),
            crate::codex_config::get_codex_model_catalog_path(),
        ],
        ManagedApp::Opencode => vec![crate::opencode_config::get_opencode_config_path()],
        ManagedApp::Gemini => vec![
            crate::gemini_config::get_gemini_env_path(),
            crate::gemini_config::get_gemini_settings_path(),
        ],
        ManagedApp::GrokBuild => vec![crate::grok_config::get_grok_config_path()],
        ManagedApp::OpenClaw => vec![crate::openclaw_config::get_openclaw_config_path()],
        ManagedApp::Hermes => vec![crate::hermes_config::get_hermes_config_path()],
        ManagedApp::Mcode => vec![crate::mcode_config::config_path()],
        ManagedApp::Pi => vec![crate::pi_config::get_pi_models_path()?],
    })
}

fn managed_app(app: &AppType) -> Option<ManagedApp> {
    ManagedApp::ALL
        .into_iter()
        .find(|managed| managed.app_type() == *app)
}

/// 首次接入前保存原文件，续费和换密钥不会覆盖尚未恢复的快照。
pub fn capture(state: &AppState, app_type: &AppType) -> Result<(), AppError> {
    let Some(app) = managed_app(app_type) else {
        return Ok(());
    };
    let _guard = SNAPSHOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut snapshots = load()?;
    if snapshots
        .get(app.as_str())
        .is_some_and(|snapshot| !snapshot.restored)
    {
        return Ok(());
    }
    if state
        .proxy_service
        .detect_takeover_in_live_config_for_app(app_type)
    {
        return Err(AppError::Message(
            "请先关闭本地路由，再切换或恢复配置".into(),
        ));
    }
    let mut contents = BTreeMap::new();
    for path in files(app)? {
        let content = match std::fs::read(&path) {
            Ok(content) => Some(content),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(AppError::io(&path, error)),
        };
        contents.insert(path, content);
    }
    // 已经接入星火的旧版本没有原始文件快照，恢复时使用原有供应商记录。
    if contents
        .values()
        .flatten()
        .any(|bytes| String::from_utf8_lossy(bytes).contains(&super::site_url()))
    {
        return Ok(());
    }
    let current = crate::settings::get_effective_current_provider(&state.db, app_type)?;
    snapshots.insert(
        app.as_str().into(),
        Snapshot {
            files: contents,
            current,
            restored: false,
        },
    );
    save(&snapshots)
}

pub fn available() -> bool {
    load().is_ok_and(|snapshots| snapshots.values().any(|snapshot| !snapshot.restored))
}

/// 恢复文件与当前供应商标记，保留激活状态和星火供应商，方便再次切回。
pub fn restore(state: &AppState) -> Vec<AppliedApp> {
    restore_selected(state, &ManagedApp::ALL)
}

pub fn restore_app(state: &AppState, app: ManagedApp) -> Result<Vec<String>, AppError> {
    let result = restore_selected(state, &[app]);
    let mut warnings = Vec::new();
    for result in result {
        if !result.ok {
            return Err(AppError::Message(
                result.error.unwrap_or_else(|| "恢复配置失败".into()),
            ));
        }
        warnings.extend(result.warnings);
    }
    Ok(warnings)
}

pub fn restore_selected(state: &AppState, selected: &[ManagedApp]) -> Vec<AppliedApp> {
    let _guard = SNAPSHOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut snapshots = match load() {
        Ok(snapshots) => snapshots,
        Err(error) => return vec![AppliedApp::from_result(ManagedApp::Claude, Err(error))],
    };
    let record = super::storage::load_activation().unwrap_or_default();
    let mut results = Vec::new();
    for &app in selected {
        let app_type = app.app_type();
        if let Some(snapshot) = snapshots
            .get_mut(app.as_str())
            .filter(|snapshot| !snapshot.restored)
        {
            let result = (|| {
                if state
                    .proxy_service
                    .detect_takeover_in_live_config_for_app(&app_type)
                {
                    return Err(AppError::Message("请先关闭本地路由，再恢复配置".into()));
                }
                if snapshot.files.keys().cloned().collect::<Vec<_>>() != {
                    let mut paths = files(app)?;
                    paths.sort();
                    paths
                } {
                    return Err(AppError::Message(
                        "配置目录已改变，请切回备份时使用的配置目录后恢复".into(),
                    ));
                }
                // 恢复前再留一份当前文件，用于任一文件写入失败时回滚。
                let mut before = BTreeMap::new();
                for path in snapshot.files.keys() {
                    let value = match std::fs::read(path) {
                        Ok(bytes) => Some(bytes),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                        Err(error) => return Err(AppError::io(path, error)),
                    };
                    before.insert(path.clone(), value);
                }
                let write =
                    |contents: &BTreeMap<PathBuf, Option<Vec<u8>>>| -> Result<(), AppError> {
                        for (path, bytes) in contents {
                            match bytes {
                                // Snapshots hold raw bytes captured before this
                                // guard existed; a dangling `model_provider`
                                // reference there would re-brick ChatGPT on
                                // restore, so sanitize Codex config.toml.
                                Some(bytes) => {
                                    let bytes = crate::codex_config::sanitize_codex_config_bytes(
                                        path, bytes,
                                    );
                                    crate::config::atomic_write_private(path, &bytes)?
                                }
                                None if app == ManagedApp::Codex
                                    && *path == crate::codex_config::get_codex_config_path() =>
                                {
                                    crate::codex_config::write_codex_live_config_atomic(Some(""))?;
                                }
                                None => match std::fs::remove_file(path) {
                                    Ok(()) => {}
                                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                                    Err(error) => return Err(AppError::io(path, error)),
                                },
                            }
                        }
                        Ok(())
                    };
                if let Err(error) = write(&snapshot.files) {
                    if let Err(rollback) = write(&before) {
                        log::error!("恢复失败且回滚失败: {rollback}");
                    }
                    return Err(error);
                }
                crate::settings::set_current_provider(&app_type, snapshot.current.as_deref())?;
                state.db.set_current_provider(
                    app_type.as_str(),
                    snapshot.current.as_deref().unwrap_or(""),
                )?;
                snapshot.restored = true;
                Ok(Vec::new())
            })();
            results.push(AppliedApp::from_result(app, result));
        } else if !app_type.is_additive_mode()
            && !snapshots.contains_key(app.as_str())
            && (record.apps.contains(&app) || record.previous.contains_key(app.as_str()))
        {
            let previous = record
                .previous
                .get(app.as_str())
                .map(String::as_str)
                .unwrap_or("default");
            if let Ok(Some(provider)) = state.db.get_provider_by_id(previous, app.as_str()) {
                let result = (|| {
                    if state
                        .proxy_service
                        .detect_takeover_in_live_config_for_app(&app_type)
                    {
                        return Err(AppError::Message("请先关闭本地路由，再恢复配置".into()));
                    }
                    crate::services::provider::write_live_snapshot(&app_type, &provider)?;
                    crate::settings::set_current_provider(&app_type, Some(previous))?;
                    state.db.set_current_provider(app.as_str(), previous)?;
                    Ok(Vec::new())
                })();
                results.push(AppliedApp::from_result(app, result));
            }
        }
    }
    if let Err(error) = save(&snapshots) {
        results.push(AppliedApp::from_result(ManagedApp::Claude, Err(error)));
    }
    if results.is_empty() {
        results.push(AppliedApp::from_result(
            selected.first().copied().unwrap_or(ManagedApp::Claude),
            Err(AppError::Message("当前工具没有可恢复的切换前配置".into())),
        ));
    }
    results
}
