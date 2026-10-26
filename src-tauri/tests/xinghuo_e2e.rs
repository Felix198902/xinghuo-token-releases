//! 端到端验证：连一个真实运行的 sub2api（本地或测试环境），用客户端代码走完
//! 方案验收的五条路径——CDK 首激、续费叠加、退出激活、重装取回、sk- 激活，
//! 中间核对 client-config 下发、各工具配置写入和余额查询。
//!
//! 默认跳过。需要一个 debug 构建能访问的服务端、一个开了「允许 Claude 客户端连接」
//! 的 OpenAI 分组，以及发到该分组、还没用过的两张卡密：
//!
//! ```text
//! XINGHUO_SITE_URL=http://127.0.0.1:18080 \
//! XINGHUO_E2E_CDKS=XH-...,XH-... \
//!   cargo test --test xinghuo_e2e -- --ignored --nocapture
//! ```

use std::path::Path;
use std::sync::Arc;

use cc_switch_lib::xinghuo::{self, ManagedApp};
use cc_switch_lib::{
    get_claude_settings_path, get_opencode_live_provider_ids, read_json_file, AppState,
};

#[path = "support.rs"]
mod support;
use support::{create_test_state, ensure_test_home, reset_test_fs, test_mutex};

async fn configure_tools(state: &Arc<AppState>, grant: xinghuo::KeyGrant) {
    let plan = xinghuo::fetch_plan(&grant.record.token).await;
    let state = state.clone();
    let result =
        tokio::task::spawn_blocking(move || xinghuo::finish_activation(&state, grant, plan))
            .await
            .expect("finish activation");
    assert!(
        result.apps.iter().all(|a| a.ok),
        "apply results: {:?}",
        result.apps
    );
}

async fn deactivate(state: &Arc<AppState>) -> xinghuo::DeactivateResult {
    let state = state.clone();
    let result = tokio::task::spawn_blocking(move || xinghuo::deactivate(&state))
        .await
        .expect("deactivate");
    assert!(
        result.apps.iter().all(|a| a.ok),
        "deactivate results: {:?}",
        result.apps
    );
    assert!(!result.view.activated);
    result
}

fn assert_tools_use(home: &Path, site: &str, token: &str) {
    let claude: serde_json::Value =
        read_json_file(&get_claude_settings_path()).expect("read claude settings");
    assert_eq!(claude["env"]["ANTHROPIC_BASE_URL"], site);
    assert_eq!(claude["env"]["ANTHROPIC_AUTH_TOKEN"], token);
    let codex = std::fs::read_to_string(home.join(".codex").join("config.toml"))
        .expect("read codex config");
    assert!(codex.contains(&format!("base_url = \"{site}/v1\"")));
    assert!(codex.contains(&format!("experimental_bearer_token = \"{token}\"")));
    let opencode = get_opencode_live_provider_ids().expect("read opencode ids");
    assert!(opencode.iter().any(|id| id == "xinghuo-opencode"));
}

fn assert_tools_released(token: &str) {
    let claude: serde_json::Value =
        read_json_file(&get_claude_settings_path()).expect("read claude settings");
    assert_ne!(claude["env"]["ANTHROPIC_AUTH_TOKEN"], token);
    let opencode = get_opencode_live_provider_ids().expect("read opencode ids");
    assert!(!opencode.iter().any(|id| id == "xinghuo-opencode"));
}

fn remaining(grant: &xinghuo::KeyGrant) -> f64 {
    grant
        .record
        .quota
        .as_ref()
        .map(|q| q.remaining)
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a running sub2api; see the module docs"]
async fn full_activation_flow_against_live_server() {
    let site = std::env::var("XINGHUO_SITE_URL").expect("set XINGHUO_SITE_URL");
    let site = site.trim_end_matches('/').to_string();
    let cdks: Vec<String> = std::env::var("XINGHUO_E2E_CDKS")
        .expect("set XINGHUO_E2E_CDKS to two unused codes")
        .split(',')
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect();
    assert!(cdks.len() >= 2, "XINGHUO_E2E_CDKS needs two unused codes");

    let _guard = test_mutex().lock().unwrap_or_else(|e| e.into_inner());
    reset_test_fs();
    let home = ensure_test_home().to_path_buf();
    let local = home.join(".xinghuo-token");
    let state = Arc::new(create_test_state().expect("create test state"));

    // 0) 输错码：不存在的激活码、拿错入口的网站兑换码，都被拒且本机不留记录
    let error = xinghuo::obtain_key("XH-ZZZZ-ZZZZ-ZZZZ-ZZZZ")
        .await
        .expect_err("unknown code must be rejected");
    assert_eq!(error.code, "CDK_NOT_FOUND", "{error}");
    let error = xinghuo::obtain_key("RD-ZZZZZ-ZZZZZ-ZZZZZ")
        .await
        .expect_err("redeem code must be rejected");
    assert_eq!(error.code, "CDK_IS_REDEEM_CODE", "{error}");
    assert!(!xinghuo::current_view().activated);
    println!("[0] 错码被拒：CDK_NOT_FOUND / CDK_IS_REDEEM_CODE");

    // 1) CDK 首激：拿到密钥，返回前已经写盘（硬规则 1）
    let grant = xinghuo::obtain_key(&cdks[0]).await.expect("activate");
    let token = grant.record.token.clone();
    assert!(token.starts_with("sk-"), "token should be an api key");
    assert!(!grant.renewal);
    assert!(
        xinghuo::current_view().activated,
        "activation must be persisted before configuring tools"
    );
    assert!(local.join("wallet.json").exists());
    let first_remaining = remaining(&grant);
    println!(
        "[1] 首激成功：{:?}，到期 {:?}，余额 {first_remaining}",
        grant.record.plan, grant.record.expires_at
    );

    // 2) 服务端下发的客户端配置
    let plan = xinghuo::fetch_plan(&token).await;
    assert!(
        plan.apps.contains(&ManagedApp::Claude),
        "group allows Claude clients"
    );
    assert!(plan.apps.contains(&ManagedApp::Codex));
    assert!(plan.apps.contains(&ManagedApp::Opencode));
    assert!(!plan.codex_model.is_empty());
    println!(
        "[2] client-config：apps={:?} codex={} opencode={:?} 单位={:?} 联系={:?} 商店={:?}",
        plan.apps,
        plan.codex_model,
        plan.opencode_models,
        plan.unit_label,
        plan.contact,
        plan.shop_url
    );

    // 3) 写入 Claude Code / Codex / OpenCode 并切换过去
    configure_tools(&state, grant).await;
    assert_tools_use(&home, &site, &token);
    println!("[3] 三个工具已指向 {site}");

    // 4) 供应商卡片的余额查询（原生模板）
    let usage = xinghuo::query_usage(&token).await.expect("usage query");
    assert!(usage.success);
    let data = usage.data.expect("usage data");
    assert_eq!(data[0].is_valid, Some(true));
    assert_eq!(data[0].remaining, Some(first_remaining));
    println!(
        "[4] 卡片余额：{:?} {:?}{:?}，{:?}",
        data[0].plan_name, data[0].unit, data[0].remaining, data[0].extra
    );

    // 5) 续费叠加：第二张卡密叠到同一把密钥，各工具配置不动
    let renewal = xinghuo::obtain_key(&cdks[1]).await.expect("renew");
    assert_eq!(
        renewal.record.token, token,
        "renewal must stack onto the same key"
    );
    assert!(renewal.renewal);
    let renewed_remaining = remaining(&renewal);
    assert!(renewed_remaining > first_remaining);
    configure_tools(&state, renewal).await;
    assert_tools_use(&home, &site, &token);
    println!("[5] 续费叠加：余额 {first_remaining} → {renewed_remaining}，密钥不变");

    // 6) 无效的 sk- 密钥被拒，本机激活不受影响；刷新状态拿到叠加后的余额
    let error = xinghuo::obtain_key("sk-this-key-does-not-exist-000000")
        .await
        .expect_err("invalid key must be rejected");
    assert_eq!(error.code, "KEY_INVALID");
    let view = xinghuo::refresh_status().await.expect("refresh status");
    assert!(view.activated);
    assert_eq!(view.valid, Some(true));
    assert_eq!(view.quota.map(|q| q.remaining), Some(renewed_remaining));
    println!("[6] 无效密钥被拒：KEY_INVALID；刷新后余额 {renewed_remaining}");

    // 7) 退出激活：各工具切回原来的供应商，星火供应商删除，钱包保留
    let result = deactivate(&state).await;
    assert!(result.view.has_wallet, "wallet must survive deactivation");
    assert_tools_released(&token);
    println!("[7] 退出激活：三个工具已切走，钱包保留");

    // 8) 重装取回：本机激活记录和钱包都没了（只剩设备号），再输一次首张卡密拿回同一把密钥
    std::fs::remove_file(local.join("wallet.json")).expect("wipe wallet");
    assert!(!xinghuo::current_view().has_wallet);
    let recovered = xinghuo::obtain_key(&cdks[0]).await.expect("recover");
    assert_eq!(
        recovered.record.token, token,
        "reinstall must recover the key"
    );
    assert_eq!(remaining(&recovered), renewed_remaining);
    configure_tools(&state, recovered).await;
    assert_tools_use(&home, &site, &token);
    deactivate(&state).await;
    println!("[8] 重装取回：同一把密钥，余额 {renewed_remaining}");

    // 9) sk- 激活：直接粘贴密钥
    let by_key = xinghuo::obtain_key(&token).await.expect("activate by key");
    assert_eq!(by_key.record.source, "key");
    assert_eq!(remaining(&by_key), renewed_remaining);
    configure_tools(&state, by_key).await;
    assert_tools_use(&home, &site, &token);
    deactivate(&state).await;
    assert_tools_released(&token);
    println!("[9] sk- 激活：三个工具已指向 {site}，退出后已切走");
}
