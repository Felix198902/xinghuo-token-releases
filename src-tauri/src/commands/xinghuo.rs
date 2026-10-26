//! 激活页命令：查询激活状态、激活 / 续费、刷新余额、退出激活。

use std::sync::LazyLock;

use tauri::{AppHandle, Manager};

use crate::store::AppState;
use crate::xinghuo::{self, ActivateResult, ActivationView, DeactivateResult, XinghuoError};

/// 激活与退出激活互斥：连点「立即激活」或激活中途点退出都只会跑一个。
static OPERATION_LOCK: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

#[tauri::command]
pub async fn discover_xinghuo_desktop_models(input: String) -> Result<Vec<String>, XinghuoError> {
    let _guard = OPERATION_LOCK.lock().await;
    xinghuo::discover_desktop_models(&input).await
}

#[tauri::command]
pub async fn configure_xinghuo_desktop(
    app_handle: AppHandle,
    input: String,
    model: Option<String>,
) -> Result<ActivateResult, XinghuoError> {
    let _guard = OPERATION_LOCK.lock().await;
    let (record, connection, plan) = xinghuo::prepare_desktop_connection(&input, model).await?;
    tauri::async_runtime::spawn_blocking(move || {
        with_state(&app_handle, |state| {
            xinghuo::finish_desktop_connection(state, record, connection, plan)
        })?
    })
    .await
    .map_err(|error| XinghuoError::new("INTERNAL", error.to_string()))?
}

fn with_state<T>(
    app_handle: &AppHandle,
    run: impl FnOnce(&AppState) -> T,
) -> Result<T, XinghuoError> {
    let state = app_handle
        .try_state::<AppState>()
        .ok_or_else(|| XinghuoError::new("STATE_UNAVAILABLE", "应用状态不可用"))?;
    Ok(run(state.inner()))
}

/// 读本机激活状态（不联网）
#[tauri::command]
pub async fn get_xinghuo_activation() -> Result<ActivationView, XinghuoError> {
    Ok(xinghuo::current_view())
}

/// 激活或续费：输入 sk- 密钥或激活码，拿到密钥后先写盘，再配置各工具。
#[tauri::command]
pub async fn activate_xinghuo(
    app_handle: AppHandle,
    input: String,
) -> Result<ActivateResult, XinghuoError> {
    let _guard = OPERATION_LOCK.lock().await;
    let grant = xinghuo::obtain_key(&input).await?;
    let plan = xinghuo::fetch_plan(&grant.record.token).await;
    tauri::async_runtime::spawn_blocking(move || {
        with_state(&app_handle, |state| {
            xinghuo::finish_activation(state, grant, plan)
        })
    })
    .await
    .map_err(|e| XinghuoError::new("INTERNAL", e.to_string()))?
}

/// 联网刷新套餐、到期与余额
#[tauri::command]
pub async fn refresh_xinghuo_status() -> Result<ActivationView, XinghuoError> {
    let _guard = OPERATION_LOCK.lock().await;
    xinghuo::refresh_status().await
}

/// 退出激活：各工具切回激活前的供应商，删除星火供应商；钱包保留。
#[tauri::command]
pub async fn deactivate_xinghuo(app_handle: AppHandle) -> Result<DeactivateResult, XinghuoError> {
    let _guard = OPERATION_LOCK.lock().await;
    tauri::async_runtime::spawn_blocking(move || with_state(&app_handle, xinghuo::deactivate))
        .await
        .map_err(|e| XinghuoError::new("INTERNAL", e.to_string()))?
}

/// 恢复切换前的配置，不清除星火激活记录。
#[tauri::command]
pub async fn restore_xinghuo_config(
    app_handle: AppHandle,
    app: Option<xinghuo::ManagedApp>,
) -> Result<DeactivateResult, XinghuoError> {
    let _guard = OPERATION_LOCK.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        with_state(&app_handle, |state| {
            let apps = match app {
                Some(app) => xinghuo::restore::restore_selected(state, &[app]),
                None => xinghuo::restore::restore(state),
            };
            DeactivateResult {
                view: xinghuo::current_view(),
                apps,
            }
        })
    })
    .await
    .map_err(|error| XinghuoError::new("INTERNAL", error.to_string()))?
}

/// 自动读取已激活的凭据，为所选工具接入固定星火接口。
#[tauri::command]
pub async fn use_xinghuo_app(
    app_handle: AppHandle,
    app: xinghuo::ManagedApp,
) -> Result<ActivateResult, XinghuoError> {
    let _guard = OPERATION_LOCK.lock().await;
    let record = xinghuo::active_record()?;
    if app == xinghuo::ManagedApp::ClaudeDesktop {
        let handle = app_handle.clone();
        let saved = record.clone();
        let connected = tauri::async_runtime::spawn_blocking(move || {
            with_state(&handle, |state| {
                xinghuo::desktop_already_connected(state, &saved)
            })
        })
        .await
        .map_err(|error| XinghuoError::new("INTERNAL", error.to_string()))??;
        if let Some(result) = connected {
            return Ok(result);
        }
    }
    let plan = xinghuo::fetch_plan(&record.token).await;
    tauri::async_runtime::spawn_blocking(move || {
        with_state(&app_handle, |state| {
            xinghuo::finish_use_app(state, record, plan, app)
        })?
    })
    .await
    .map_err(|error| XinghuoError::new("INTERNAL", error.to_string()))?
}
