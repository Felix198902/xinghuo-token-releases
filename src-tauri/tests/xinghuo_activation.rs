use serde_json::json;

use cc_switch_lib::xinghuo::{
    apply_providers, deactivate_providers, ActivationRecord, ApplyPlan, ManagedApp, SITE_URL,
    USAGE_TEMPLATE_TYPE,
};
use cc_switch_lib::{
    get_claude_settings_path, get_opencode_live_provider_ids, read_json_file, AppState, AppType,
    Provider, ProviderService,
};

#[path = "support.rs"]
mod support;
use support::{create_test_state, ensure_test_home, reset_test_fs, test_mutex};

fn current(state: &AppState, app_type: AppType) -> String {
    ProviderService::current(state, app_type).expect("read current provider")
}

fn has_provider(state: &AppState, app_type: AppType, id: &str) -> bool {
    ProviderService::list(state, app_type)
        .expect("list providers")
        .contains_key(id)
}

/// 星火供应商的用量配置：(模板类型, 自动查询间隔)
fn usage_config(state: &AppState, app_type: AppType, id: &str) -> Option<(String, Option<u64>)> {
    ProviderService::list(state, app_type)
        .expect("list providers")
        .get(id)
        .and_then(|p| p.meta.as_ref())
        .and_then(|m| m.usage_script.as_ref())
        .map(|s| {
            (
                s.template_type.clone().unwrap_or_default(),
                s.auto_query_interval,
            )
        })
}

fn claude_env() -> serde_json::Value {
    let settings: serde_json::Value =
        read_json_file(&get_claude_settings_path()).expect("read claude settings");
    settings["env"].clone()
}

/// Codex 的密钥以 `experimental_bearer_token` 写在 config.toml 的供应商表里。
fn codex_config() -> String {
    std::fs::read_to_string(ensure_test_home().join(".codex").join("config.toml"))
        .expect("read codex config")
}

/// 用户激活前已有自己的 Claude 供应商，Codex 什么都没有。
fn seed_user_claude_provider(state: &AppState) {
    let provider = Provider::with_id(
        "my-claude".to_string(),
        "My Claude".to_string(),
        json!({ "env": {
            "ANTHROPIC_BASE_URL": "https://api.example.com",
            "ANTHROPIC_AUTH_TOKEN": "user-own-token",
        }}),
        None,
    );
    ProviderService::add(state, AppType::Claude, provider, true).expect("seed claude provider");
    assert_eq!(current(state, AppType::Claude), "my-claude");
}

#[test]
fn activation_configures_all_four_tools_and_deactivation_restores_them() {
    let _guard = test_mutex().lock().unwrap_or_else(|e| e.into_inner());
    reset_test_fs();
    ensure_test_home();
    let state = create_test_state().expect("create test state");
    seed_user_claude_provider(&state);
    tauri::async_runtime::block_on(async {
        let mut config = state.db.get_global_proxy_config().await.unwrap();
        config.listen_port = 0;
        state.db.update_global_proxy_config(config).await.unwrap();
    });

    // 首次激活
    let mut record = ActivationRecord {
        token: "sk-first-key-000000".to_string(),
        ..Default::default()
    };
    let plan = ApplyPlan {
        claude_desktop_models: vec!["claude-sonnet-5".into(), "claude-opus-5".into()],
        ..ApplyPlan::defaults()
    };
    let applied = apply_providers(&state, "sk-first-key-000000", &plan, false, &mut record);
    assert!(applied.iter().all(|a| a.ok), "apply results: {applied:?}");
    assert_eq!(applied.len(), 4);
    let desktop = ProviderService::list(&state, AppType::ClaudeDesktop).unwrap()
        ["xinghuo-claude-desktop"]
        .clone();
    let meta = desktop.meta.unwrap();
    assert_eq!(meta.api_format.as_deref(), Some("anthropic"));
    assert_eq!(meta.claude_desktop_model_routes.len(), 2);
    for (id, route) in meta.claude_desktop_model_routes {
        assert_eq!(id, route.model);
        assert!(!route.model.contains("gpt"));
    }
    assert_eq!(
        current(&state, AppType::ClaudeDesktop),
        "xinghuo-claude-desktop"
    );

    assert_eq!(current(&state, AppType::Claude), "xinghuo-claude");
    assert_eq!(current(&state, AppType::Codex), "xinghuo-codex");
    assert_eq!(
        record.previous.get("claude").map(String::as_str),
        Some("my-claude")
    );
    assert!(!record.previous.contains_key("codex"));

    let env = claude_env();
    assert_eq!(env["ANTHROPIC_BASE_URL"], SITE_URL);
    assert_eq!(env["ANTHROPIC_AUTH_TOKEN"], "sk-first-key-000000");

    let config = codex_config();
    assert!(config.contains("experimental_bearer_token = \"sk-first-key-000000\""));
    assert!(config.contains("base_url = \"https://ai.wfpromax.xyz/v1\""));
    assert!(config.contains("model = \"gpt-5.5\""));

    let opencode_ids = get_opencode_live_provider_ids().expect("read opencode live ids");
    assert!(opencode_ids.iter().any(|id| id == "xinghuo-opencode"));

    // 每个星火供应商都带上原生余额查询，5 分钟刷新一次
    for app in [
        ManagedApp::Claude,
        ManagedApp::ClaudeDesktop,
        ManagedApp::Codex,
        ManagedApp::Opencode,
    ] {
        assert_eq!(
            usage_config(&state, app.app_type(), app.provider_id()),
            Some((USAGE_TEMPLATE_TYPE.to_string(), Some(5))),
            "{} usage script",
            app.provider_id()
        );
    }

    // 续费拿回同一把密钥：配置保持不动（这里用不同的计划验证没有被重写）
    let mut renewal_plan = ApplyPlan::defaults();
    renewal_plan.codex_model = "gpt-renewal-should-not-apply".to_string();
    let applied = apply_providers(
        &state,
        "sk-first-key-000000",
        &renewal_plan,
        true,
        &mut record,
    );
    assert!(applied.iter().all(|a| a.ok));
    assert!(!codex_config().contains("gpt-renewal-should-not-apply"));

    // 换密钥：各工具改用新密钥，激活前的供应商记录不被覆盖
    let applied = apply_providers(&state, "sk-second-key-11111", &plan, false, &mut record);
    assert!(applied.iter().all(|a| a.ok));
    assert_eq!(claude_env()["ANTHROPIC_AUTH_TOKEN"], "sk-second-key-11111");
    assert!(codex_config().contains("experimental_bearer_token = \"sk-second-key-11111\""));
    assert_eq!(
        record.previous.get("claude").map(String::as_str),
        Some("my-claude")
    );

    // 退出激活：Claude 切回用户原来的供应商，Codex 没有原供应商时清除认证
    let results = deactivate_providers(&state, &record);
    assert!(
        results.iter().all(|a| a.ok),
        "deactivate results: {results:?}"
    );
    assert_eq!(current(&state, AppType::Claude), "my-claude");
    assert_eq!(claude_env()["ANTHROPIC_AUTH_TOKEN"], "user-own-token");
    assert_eq!(current(&state, AppType::Codex), "");
    assert!(!codex_config().contains("sk-second-key-11111"));
    assert!(!has_provider(&state, AppType::Codex, "codex-official"));
    for app in ManagedApp::ALL {
        assert!(
            !has_provider(&state, app.app_type(), app.provider_id()),
            "{} should be removed",
            app.provider_id()
        );
    }
    let opencode_ids = get_opencode_live_provider_ids().expect("read opencode live ids");
    assert!(!opencode_ids.iter().any(|id| id == "xinghuo-opencode"));
    tauri::async_runtime::block_on(state.proxy_service.stop()).expect("stop desktop proxy");
}

#[test]
fn plan_can_limit_which_tools_are_configured() {
    let _guard = test_mutex().lock().unwrap_or_else(|e| e.into_inner());
    reset_test_fs();
    ensure_test_home();
    let state = create_test_state().expect("create test state");

    let mut record = ActivationRecord::default();
    let plan = ApplyPlan {
        apps: vec![ManagedApp::Codex],
        ..ApplyPlan::defaults()
    };
    let applied = apply_providers(&state, "sk-codex-only-2222", &plan, false, &mut record);
    assert_eq!(applied.len(), 1);
    assert!(applied[0].ok);
    assert!(has_provider(&state, AppType::Codex, "xinghuo-codex"));
    assert!(!has_provider(&state, AppType::Claude, "xinghuo-claude"));

    // 没有任何星火供应商时退出激活也不报错
    let results = deactivate_providers(&state, &ActivationRecord::default());
    assert!(results.iter().all(|a| a.ok));
    assert!(!has_provider(&state, AppType::Codex, "xinghuo-codex"));
    let results = deactivate_providers(&state, &ActivationRecord::default());
    assert!(results.iter().all(|a| a.ok));
}

#[test]
fn activation_configures_extra_platforms_and_deactivation_cleans_up() {
    let _guard = test_mutex().lock().unwrap_or_else(|e| e.into_inner());
    reset_test_fs();
    let home = ensure_test_home().to_path_buf();
    // 这些平台的数据目录可以被环境变量改道，测试期间一律指回测试 HOME。
    let redirected = [
        "HERMES_HOME",
        "MINIMAX_DATA_DIR",
        "MAVIS_DATA_DIR",
        "PI_CODING_AGENT_DIR",
    ];
    let saved: Vec<_> = redirected
        .iter()
        .map(|key| (*key, std::env::var_os(key)))
        .collect();
    for key in redirected {
        std::env::remove_var(key);
    }
    for sub in [".hermes", ".minimax", ".pi"] {
        let _ = std::fs::remove_dir_all(home.join(sub));
    }
    let state = create_test_state().expect("create test state");

    let extra = [
        ManagedApp::Gemini,
        ManagedApp::GrokBuild,
        ManagedApp::OpenClaw,
        ManagedApp::Hermes,
        ManagedApp::Mcode,
        ManagedApp::Pi,
    ];
    let mut plan = ApplyPlan {
        apps: extra.to_vec(),
        ..ApplyPlan::defaults()
    };
    for app in extra {
        plan.extra_models.insert(app, vec!["gpt-5.5".to_string()]);
    }
    let mut record = ActivationRecord {
        token: "sk-extra-00000".to_string(),
        ..Default::default()
    };
    let applied = apply_providers(&state, "sk-extra-00000", &plan, false, &mut record);
    assert!(applied.iter().all(|a| a.ok), "apply results: {applied:?}");
    assert_eq!(applied.len(), 6);
    for app in extra {
        assert!(
            has_provider(&state, app.app_type(), app.provider_id()),
            "{} should exist",
            app.provider_id()
        );
        assert_eq!(
            usage_config(&state, app.app_type(), app.provider_id()),
            Some((USAGE_TEMPLATE_TYPE.to_string(), Some(5))),
            "{} usage script",
            app.provider_id()
        );
    }

    let gemini_env = std::fs::read_to_string(home.join(".gemini/.env")).expect("gemini env");
    assert!(gemini_env.contains("GEMINI_API_KEY=sk-extra-00000"));
    assert!(gemini_env.contains("GOOGLE_GEMINI_BASE_URL=https://ai.wfpromax.xyz"));
    let grok = std::fs::read_to_string(home.join(".grok/config.toml")).expect("grok config");
    assert!(grok.contains("api_key = \"sk-extra-00000\""));
    assert!(grok.contains("base_url = \"https://ai.wfpromax.xyz/v1\""));
    let openclaw =
        std::fs::read_to_string(home.join(".openclaw/openclaw.json")).expect("openclaw config");
    assert!(openclaw.contains("xinghuo-openclaw"));
    let hermes = std::fs::read_to_string(home.join(".hermes/config.yaml")).expect("hermes config");
    assert!(hermes.contains("sk-extra-00000"));
    let mcode = std::fs::read_to_string(home.join(".minimax/config.yaml")).expect("mcode config");
    assert!(mcode.contains("sk-extra-00000"));
    let pi = std::fs::read_to_string(home.join(".pi/agent/models.json")).expect("pi models");
    assert!(pi.contains("xinghuo-pi"));

    // 退出激活：六个平台的星火供应商全部删除，切换型平台的凭据被清除
    let results = deactivate_providers(&state, &record);
    assert!(
        results.iter().all(|a| a.ok),
        "deactivate results: {results:?}"
    );
    for app in extra {
        assert!(
            !has_provider(&state, app.app_type(), app.provider_id()),
            "{} should be removed",
            app.provider_id()
        );
    }
    let gemini_env = std::fs::read_to_string(home.join(".gemini/.env")).expect("gemini env");
    assert!(!gemini_env.contains("sk-extra-00000"));
    let grok = std::fs::read_to_string(home.join(".grok/config.toml")).expect("grok config");
    assert!(!grok.contains("sk-extra-00000"));
    let openclaw =
        std::fs::read_to_string(home.join(".openclaw/openclaw.json")).expect("openclaw config");
    assert!(!openclaw.contains("xinghuo-openclaw"));
    let hermes = std::fs::read_to_string(home.join(".hermes/config.yaml")).expect("hermes config");
    assert!(!hermes.contains("sk-extra-00000"));
    let mcode = std::fs::read_to_string(home.join(".minimax/config.yaml")).expect("mcode config");
    assert!(!mcode.contains("sk-extra-00000"));
    let pi = std::fs::read_to_string(home.join(".pi/agent/models.json")).expect("pi models");
    assert!(!pi.contains("xinghuo-pi"));

    for (key, value) in saved {
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}

#[test]
fn desktop_missing_catalog_does_not_write_configuration_or_inherit_codex() {
    let _guard = test_mutex().lock().unwrap_or_else(|e| e.into_inner());
    reset_test_fs();
    let state = create_test_state().unwrap();
    let mut record = ActivationRecord::default();
    let plan = ApplyPlan {
        apps: vec![ManagedApp::ClaudeDesktop],
        ..ApplyPlan::defaults()
    };
    let result = apply_providers(&state, "sk-test", &plan, false, &mut record);
    assert!(!result[0].ok);
    assert!(result[0].error.as_ref().unwrap().contains("没有返回可用"));
    assert!(!has_provider(
        &state,
        AppType::ClaudeDesktop,
        "xinghuo-claude-desktop"
    ));
    assert!(!ensure_test_home()
        .join(".xinghuo-token/restore.json")
        .exists());
}

#[test]
fn desktop_model_quota_error_is_not_reported_as_empty_catalog() {
    let _guard = test_mutex().lock().unwrap_or_else(|e| e.into_inner());
    reset_test_fs();
    let state = create_test_state().unwrap();
    let mut record = ActivationRecord::default();
    let plan = ApplyPlan {
        apps: vec![ManagedApp::ClaudeDesktop],
        claude_desktop_model_error: Some(cc_switch_lib::xinghuo::XinghuoError::with_status(
            "API_KEY_QUOTA_EXHAUSTED",
            429,
            "API key 额度已用完",
        )),
        ..ApplyPlan::defaults()
    };
    let result = apply_providers(&state, "sk-test", &plan, false, &mut record);
    assert!(!result[0].ok);
    assert!(result[0].error.as_ref().unwrap().contains("额度已用完"));
    assert!(!has_provider(
        &state,
        AppType::ClaudeDesktop,
        "xinghuo-claude-desktop"
    ));
}

#[test]
fn desktop_native_catalog_routes_all_models_and_preserves_streaming() {
    desktop_catalog_http_test("anthropic");
}

#[test]
fn desktop_responses_bridge_routes_all_models_and_translates_streaming() {
    desktop_catalog_http_test("openai_responses");
}

fn desktop_catalog_http_test(api_format: &str) {
    use axum::{routing::post, Json, Router};
    use serde_json::Value;
    use std::sync::{Arc, Mutex};
    let _guard = test_mutex().lock().unwrap_or_else(|e| e.into_inner());
    reset_test_fs();
    let state = create_test_state().unwrap();
    let captured = Arc::new(Mutex::new(Vec::<Value>::new()));
    let responses = api_format == "openai_responses";
    let (upstream_url, task) = tauri::async_runtime::block_on(async {
        let capture = captured.clone();
        let app = Router::new().route(if responses {"/v1/responses"} else {"/v1/messages"}, post(move |headers: axum::http::HeaderMap, Json(body): Json<Value>| {
            let capture = capture.clone();
            async move {
                assert_eq!(headers["authorization"], "Bearer sk-native-test");
                capture.lock().unwrap().push(body.clone());
                if responses {
                    let item = json!({"id":"item_text","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"OK","annotations":[]}]});
                    let response = json!({"id":"resp_test","object":"response","model":body["model"],"status":"completed","output":[item],"usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}});
                    let events = [
                        json!({"type":"response.created","response":{"id":"resp_test","model":body["model"],"status":"in_progress","output":[]}}),
                        json!({"type":"response.output_item.added","output_index":0,"item":{"id":"item_text","type":"message","role":"assistant","content":[]}}),
                        json!({"type":"response.content_part.added","output_index":0,"content_index":0,"item_id":"item_text","part":{"type":"output_text","text":"","annotations":[]}}),
                        json!({"type":"response.output_text.delta","output_index":0,"content_index":0,"item_id":"item_text","delta":"OK"}),
                        json!({"type":"response.output_item.done","output_index":0,"item":item}),
                        json!({"type":"response.completed","response":response})
                    ];
                    let text = events.iter().map(|event| format!("event: {}\ndata: {event}\n\n",event["type"].as_str().unwrap())).collect::<String>();
                    return ([("content-type", "text/event-stream")], text);
                }
                let message = json!({"id":"msg_native", "type":"message", "role":"assistant", "model":body["model"], "content":[], "stop_reason":null,"usage":{"input_tokens":1,"output_tokens":0}});
                if body["stream"] == true {
                    ([("content-type", "text/event-stream")], format!("event: message_start\ndata: {}\n\nevent: content_block_start\ndata: {}\n\nevent: content_block_delta\ndata: {}\n\nevent: content_block_stop\ndata: {}\n\nevent: message_delta\ndata: {}\n\nevent: message_stop\ndata: {{\"type\":\"message_stop\"}}\n\n",
                        json!({"type":"message_start","message":message}),
                        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
                        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"OK"}}),
                        json!({"type":"content_block_stop","index":0}),
                        json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":1}})))
                } else {
                    let mut message = message;
                    message["content"] = json!([{"type":"text","text":"OK"}]);
                    message["stop_reason"] = json!("end_turn");
                    ([("content-type", "application/json")], message.to_string())
                }
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut config = state.db.get_global_proxy_config().await.unwrap();
        config.listen_port = 0;
        state.db.update_global_proxy_config(config).await.unwrap();
        (format!("http://{addr}/v1"), task)
    });
    let ids = vec![
        "claude-sonnet-5".to_string(),
        "claude-opus-4-8".to_string(),
        "deepseek-v4-pro".to_string(),
        "hermes-3".to_string(),
    ];
    let plan = ApplyPlan {
        apps: vec![ManagedApp::ClaudeDesktop],
        claude_desktop_models: ids.clone(),
        claude_desktop_api_format: api_format.into(),
        ..ApplyPlan::defaults()
    };
    let applied = apply_providers(
        &state,
        "sk-native-test",
        &plan,
        false,
        &mut ActivationRecord::default(),
    );
    assert!(applied[0].ok, "{applied:?}");
    let mut provider = ProviderService::list(&state, AppType::ClaudeDesktop).unwrap()
        ["xinghuo-claude-desktop"]
        .clone();
    provider.settings_config["env"]["ANTHROPIC_BASE_URL"] = json!(upstream_url);
    ProviderService::update(&state, AppType::ClaudeDesktop, None, provider).unwrap();
    let snapshots: Value =
        read_json_file(&ensure_test_home().join(".xinghuo-token/restore.json")).unwrap();
    let path = snapshots["claude-desktop"]["files"]
        .as_object()
        .unwrap()
        .keys()
        .find(|p| p.contains("configLibrary") && !p.ends_with("meta.json"))
        .unwrap();
    let profile: Value = read_json_file(std::path::Path::new(path)).unwrap();
    tauri::async_runtime::block_on(async {
        let client = reqwest::Client::new();
        let base = profile["inferenceGatewayBaseUrl"].as_str().unwrap();
        let token = profile["inferenceGatewayApiKey"].as_str().unwrap();
        let models: Value = client
            .get(format!("{base}/v1/models"))
            .header("x-api-key", token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(models["data"].as_array().unwrap().len(), ids.len());
        for (index, id) in ids.iter().enumerate() {
            let route = models["data"]
                .as_array()
                .unwrap()
                .iter()
                .find(|model| model["id"] == *id || model["display_name"] == *id)
                .unwrap_or_else(|| panic!("missing desktop route for {id}: {models}"));
            let request = client.post(format!("{base}/v1/messages"));
            let request = if index % 2 == 0 {
                request.bearer_auth(token)
            } else {
                request.header("x-api-key", token)
            };
            let response = request
                .json(&json!({
                    "model":route["id"],"max_tokens":16,"stream":true,
                    "system":[{"type":"text","text":"You are a desktop assistant, not Claude Code."}],
                    "tool_choice":{"type":"any"},
                    "tools":[{"name":"lookup","input_schema":{"type":"object"}}],
                    "messages":[{"role":"user","content":"Reply OK"}]
                }))
                .send()
                .await
                .unwrap();
            assert!(response.status().is_success());
            let text = response.text().await.unwrap();
            assert!(
                text.contains("message_stop") && text.contains("OK"),
                "{text}"
            );
        }
        state.proxy_service.stop().await.unwrap();
    });
    let requests = captured.lock().unwrap();
    assert_eq!(requests.len(), ids.len());
    for (body, id) in requests.iter().zip(ids) {
        assert_eq!(body["model"], id);
        assert_eq!(body["tools"][0]["name"], "lookup");
        assert!(body["tools"][0].get("function").is_none());
        if responses {
            assert_eq!(
                body["instructions"],
                "You are a desktop assistant, not Claude Code."
            );
            assert_eq!(body["tool_choice"], "required");
        } else {
            assert_eq!(
                body["system"][0]["text"],
                "You are a desktop assistant, not Claude Code."
            );
            assert_eq!(body["tool_choice"], json!({"type":"any"}));
        }
    }
    task.abort();
}

#[test]
fn desktop_gateway_bridges_auth_models_tools_streaming_and_restores_files() {
    use axum::{routing::post, Json, Router};
    use serde_json::Value;
    use std::sync::{Arc, Mutex};

    let _guard = test_mutex().lock().unwrap_or_else(|e| e.into_inner());
    reset_test_fs();
    let state = create_test_state().unwrap();
    #[cfg(target_os = "macos")]
    let desktop_dir = ensure_test_home().join("Library/Application Support/Claude");
    #[cfg(windows)]
    let desktop_dir = ensure_test_home().join("AppData/Local/Claude");
    #[cfg(target_os = "linux")]
    let desktop_dir = ensure_test_home().join(".config/Claude");
    std::fs::create_dir_all(&desktop_dir).unwrap();
    let original_path = desktop_dir.join("claude_desktop_config.json");
    let original = br#"{"deploymentMode":"1p","mcpServers":{"existing":{"command":"local-tool"}}}"#;
    std::fs::write(&original_path, original).unwrap();

    // 桌面配置切换与恢复不能触碰两种模式下的历史会话或资料。
    let preserved_paths = [
        desktop_dir.join("local-agent-mode-sessions/history.json"),
        desktop_dir
            .with_file_name("Claude-3p")
            .join("local-agent-mode-sessions/history.json"),
    ];
    for path in &preserved_paths {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"existing user conversation and materials").unwrap();
    }

    let captured = Arc::new(Mutex::new(Vec::<(String, Value)>::new()));
    let (upstream_url, upstream_task) = tauri::async_runtime::block_on(async {
        let capture = captured.clone();
        let app = Router::new().route("/v1/chat/completions", post(
            move |headers: axum::http::HeaderMap, Json(body): Json<Value>| {
                let capture = capture.clone();
                async move {
                    capture.lock().unwrap().push((headers["authorization"].to_str().unwrap().into(), body.clone()));
                    if body["stream"] == true {
                        ([("content-type", "text/event-stream")], concat!(
                            "data: {\"id\":\"chat-test\",\"object\":\"chat.completion.chunk\",\"model\":\"test-upstream\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"hello\"},\"finish_reason\":null}]}\n\n",
                            "data: {\"id\":\"chat-test\",\"object\":\"chat.completion.chunk\",\"model\":\"test-upstream\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_test\",\"type\":\"function\",\"function\":{\"name\":\"lookup\",\"arguments\":\"{\\\"query\\\":\\\"test\\\"}\"}}]},\"finish_reason\":null}]}\n\n",
                            "data: {\"id\":\"chat-test\",\"object\":\"chat.completion.chunk\",\"model\":\"test-upstream\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"total_tokens\":15}}\n\n",
                            "data: [DONE]\n\n"
                        ).to_string())
                    } else {
                        ([("content-type", "application/json")], json!({
                            "id":"chat-test", "object":"chat.completion", "model":"test-upstream",
                            "choices":[{"index":0,"message":{"role":"assistant","content":null,"tool_calls":[{
                                "id":"call_test","type":"function","function":{"name":"lookup","arguments":"{\"query\":\"test\"}"}
                            }]},"finish_reason":"tool_calls"}],
                            "usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}
                        }).to_string())
                    }
                }
            }
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut config = state.db.get_global_proxy_config().await.unwrap();
        config.listen_address = "127.0.0.1".into();
        config.listen_port = 0;
        state.db.update_global_proxy_config(config).await.unwrap();
        (format!("http://{addr}/v1"), task)
    });
    let plan = ApplyPlan {
        apps: vec![ManagedApp::ClaudeDesktop],
        claude_desktop_model: Some("test-upstream".into()),
        claude_desktop_api_format: "openai_chat".into(),
        claude_desktop_token: Some("sk-upstream-test".into()),
        ..ApplyPlan::defaults()
    };
    let record = ActivationRecord {
        token: "sk-primary-test".into(),
        ..Default::default()
    };
    let result = cc_switch_lib::xinghuo::finish_desktop_connection(
        &state,
        record,
        cc_switch_lib::xinghuo::DesktopConnection {
            token: "sk-upstream-test".into(),
            api_format: "openai_chat".into(),
            model: Some("test-upstream".into()),
        },
        plan,
    )
    .unwrap();
    assert!(result.apps[0].ok);
    let saved = cc_switch_lib::xinghuo::active_record().unwrap();
    assert_eq!(saved.token, "sk-primary-test");
    assert_eq!(saved.desktop_connection.unwrap().token, "sk-upstream-test");
    assert!(result.view.desktop_connection_configured);
    let id = ManagedApp::ClaudeDesktop.provider_id();
    let mut provider = ProviderService::list(&state, AppType::ClaudeDesktop).unwrap()[id].clone();
    provider.settings_config["env"]["ANTHROPIC_BASE_URL"] = json!(upstream_url);
    ProviderService::update(&state, AppType::ClaudeDesktop, None, provider).unwrap();
    let snapshots: Value =
        read_json_file(&ensure_test_home().join(".xinghuo-token/restore.json")).unwrap();
    let files = snapshots["claude-desktop"]["files"].as_object().unwrap();
    let profile_path = files
        .keys()
        .find(|path| path.contains("configLibrary") && !path.ends_with("meta.json"))
        .unwrap();
    let profile: Value = read_json_file(std::path::Path::new(profile_path)).unwrap();
    assert_eq!(profile["inferenceProvider"], "gateway");
    let local_token = profile["inferenceGatewayApiKey"].as_str().unwrap();
    assert_ne!(local_token, "sk-upstream-test");
    let base = profile["inferenceGatewayBaseUrl"].as_str().unwrap();
    tauri::async_runtime::block_on(async {
        let client = reqwest::Client::new();
        assert_eq!(
            client
                .get(format!("{base}/v1/models"))
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
        let models: Value = client
            .get(format!("{base}/v1/models"))
            .bearer_auth(local_token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let route = models["data"][0]["id"].as_str().unwrap();
        for stream in [false, true] {
            let response = client.post(format!("{base}/v1/messages")).bearer_auth(local_token).json(&json!({
                "model":route,"max_tokens":128,"stream":stream,
                "system":[{"type":"text","text":"You are a desktop assistant, not Claude Code."}],
                "tool_choice":{"type":"any"},
                "tools":[{"name":"lookup","description":"lookup","input_schema":{"type":"object","properties":{"query":{"type":"string"}}}}],
                "messages":[
                    {"role":"user","content":[{"type":"text","text":"desktop reminder"},{"type":"text","text":"lookup test"}]},
                    {"role":"assistant","content":[{"type":"tool_use","id":"call_previous","name":"lookup","input":{"query":"previous"}}]},
                    {"role":"user","content":[{"type":"tool_result","tool_use_id":"call_previous","content":"previous result"}]}
                ]
            })).send().await.unwrap();
            assert!(
                response.status().is_success(),
                "{}",
                response.text().await.unwrap()
            );
            let text = response.text().await.unwrap();
            if stream {
                assert!(
                    text.contains("message_start") && text.contains("message_stop"),
                    "{text}"
                );
                assert!(
                    text.contains("hello")
                        && text.contains("input_json_delta")
                        && text.contains("tool_use"),
                    "{text}"
                );
            } else {
                let body: Value = serde_json::from_str(&text).unwrap();
                assert_eq!(body["stop_reason"], "tool_use");
                assert_eq!(body["content"][0]["input"]["query"], "test");
            }
        }
        state.proxy_service.stop().await.unwrap();
    });
    let requests = captured.lock().unwrap();
    assert_eq!(requests.len(), 2);
    for (auth, body) in requests.iter() {
        assert_eq!(auth, "Bearer sk-upstream-test");
        assert_eq!(body["model"], "test-upstream");
        assert_eq!(body["tools"][0]["function"]["name"], "lookup");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(
            body["messages"][0]["content"],
            "You are a desktop assistant, not Claude Code."
        );
        assert_eq!(body["tool_choice"], "required");
        assert_eq!(
            body["messages"][1]["content"],
            "desktop reminder\n\nlookup test"
        );
        assert_eq!(body["messages"][3]["role"], "tool");
        assert_eq!(body["messages"][3]["tool_call_id"], "call_previous");
    }
    let restored = cc_switch_lib::xinghuo::restore::restore(&state);
    assert!(restored.iter().all(|result| result.ok), "{restored:?}");
    assert_eq!(std::fs::read(&original_path).unwrap(), original);
    assert!(!std::path::Path::new(profile_path).exists());
    assert_eq!(current(&state, AppType::ClaudeDesktop), "");
    for path in &preserved_paths {
        assert_eq!(
            std::fs::read(path).unwrap(),
            b"existing user conversation and materials"
        );
    }
    upstream_task.abort();
}

#[test]
fn renewal_backfills_usage_script_without_touching_tool_config() {
    let _guard = test_mutex().lock().unwrap_or_else(|e| e.into_inner());
    reset_test_fs();
    ensure_test_home();
    let state = create_test_state().expect("create test state");

    let mut record = ActivationRecord::default();
    let plan = ApplyPlan {
        apps: vec![ManagedApp::Claude],
        ..ApplyPlan::defaults()
    };
    let applied = apply_providers(&state, "sk-backfill-key-333", &plan, false, &mut record);
    assert!(applied.iter().all(|a| a.ok));

    // 模拟老版本建的供应商：没有余额查询配置
    let mut old = ProviderService::list(&state, AppType::Claude)
        .expect("list providers")
        .get("xinghuo-claude")
        .cloned()
        .expect("xinghuo-claude exists");
    old.meta.as_mut().expect("meta").usage_script = None;
    ProviderService::update(&state, AppType::Claude, None, old).expect("strip usage script");
    assert_eq!(
        usage_config(&state, AppType::Claude, "xinghuo-claude"),
        None
    );

    let applied = apply_providers(&state, "sk-backfill-key-333", &plan, true, &mut record);
    assert!(applied.iter().all(|a| a.ok));
    assert_eq!(
        usage_config(&state, AppType::Claude, "xinghuo-claude"),
        Some((USAGE_TEMPLATE_TYPE.to_string(), Some(5)))
    );
    assert_eq!(claude_env()["ANTHROPIC_AUTH_TOKEN"], "sk-backfill-key-333");
    assert_eq!(current(&state, AppType::Claude), "xinghuo-claude");
}

#[test]
fn legacy_official_and_default_are_unavailable_and_default_is_not_backfilled() {
    let _guard = test_mutex().lock().unwrap_or_else(|e| e.into_inner());
    reset_test_fs();
    let state = create_test_state().expect("create test state");
    let original = json!({"env": {"ANTHROPIC_AUTH_TOKEN": "saved-default"}});
    let default = Provider::with_id("default".into(), "default".into(), original.clone(), None);
    state.db.save_provider("claude", &default).unwrap();
    state.db.set_current_provider("claude", "default").unwrap();
    let mut official = Provider::with_id(
        "claude-official".into(),
        "Official".into(),
        json!({"env":{}}),
        None,
    );
    official.category = Some("official".into());
    state.db.save_provider("claude", &official).unwrap();
    assert!(!has_provider(&state, AppType::Claude, "default"));
    assert!(!has_provider(&state, AppType::Claude, "claude-official"));
    assert!(ProviderService::switch(&state, AppType::Claude, "claude-official").is_err());
    assert!(ProviderService::add(&state, AppType::Claude, official, false).is_err());
    let path = get_claude_settings_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({"env":{"ANTHROPIC_AUTH_TOKEN":"live-changed"}})).unwrap(),
    )
    .unwrap();
    let mut record = ActivationRecord::default();
    let plan = ApplyPlan {
        apps: vec![ManagedApp::Claude],
        ..ApplyPlan::defaults()
    };
    assert!(
        apply_providers(&state, "sk-test-activation", &plan, false, &mut record)
            .iter()
            .all(|app| app.ok)
    );
    assert_eq!(
        state
            .db
            .get_provider_by_id("default", "claude")
            .unwrap()
            .unwrap()
            .settings_config,
        original
    );
}

#[test]
fn restore_recovers_original_files_and_keeps_xinghuo_provider() {
    let _guard = test_mutex().lock().unwrap_or_else(|e| e.into_inner());
    reset_test_fs();
    let state = create_test_state().unwrap();
    let path = get_claude_settings_path();
    let original = "{\"env\":{\"ANTHROPIC_AUTH_TOKEN\":\"original-key\"},\"custom\":true}\n";
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, original).unwrap();
    let mut record = ActivationRecord::default();
    let plan = ApplyPlan {
        apps: vec![ManagedApp::Claude, ManagedApp::Codex],
        ..ApplyPlan::defaults()
    };
    assert!(
        apply_providers(&state, "sk-restore-test", &plan, false, &mut record)
            .iter()
            .all(|app| app.ok)
    );
    let result = cc_switch_lib::xinghuo::restore::restore(&state);
    assert!(result.iter().all(|app| app.ok), "{result:?}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    let restored: toml::Value = codex_config().parse().unwrap();
    assert!(restored.get("model_provider").is_none());
    assert_eq!(
        restored["model_providers"]["custom"]["name"].as_str(),
        Some("OpenAI")
    );
    assert!(restored["model_providers"]["custom"]
        .get("experimental_bearer_token")
        .is_none());
    assert!(has_provider(&state, AppType::Claude, "xinghuo-claude"));
    assert!(!cc_switch_lib::xinghuo::restore::available());
}

#[test]
fn legacy_restore_uses_existing_default_without_changing_activation() {
    let _guard = test_mutex().lock().unwrap_or_else(|e| e.into_inner());
    reset_test_fs();
    let state = create_test_state().unwrap();
    let mut default = Provider::with_id(
        "default".into(),
        "default".into(),
        json!({"auth":{}, "config":"model_provider = \"openai\"\n"}),
        None,
    );
    default.category = Some("official".into());
    state.db.save_provider("codex", &default).unwrap();
    let record = ActivationRecord {
        token: "sk-retained".into(),
        apps: vec![ManagedApp::Codex],
        previous: std::collections::BTreeMap::from([("codex".into(), "default".into())]),
        ..Default::default()
    };
    let dir = ensure_test_home().join(".xinghuo-token");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("activation.json"),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();
    let result = cc_switch_lib::xinghuo::restore::restore(&state);
    assert_eq!(result.len(), 1);
    assert!(result[0].ok, "{result:?}");
    assert_eq!(current(&state, AppType::Codex), "default");
    assert!(codex_config().contains("model_provider = \"openai\""));
    assert!(cc_switch_lib::xinghuo::current_view().activated);
    assert!(!has_provider(&state, AppType::Codex, "codex-official"));
}

#[test]
fn one_click_reapplies_only_selected_tool_and_can_restore_again() {
    let _guard = test_mutex().lock().unwrap_or_else(|e| e.into_inner());
    reset_test_fs();
    let home = ensure_test_home();
    let state = create_test_state().expect("create state");
    let codex_dir = home.join(".codex");
    std::fs::create_dir_all(&codex_dir).unwrap();
    let original = "model = \"official-model\"\n";
    std::fs::write(codex_dir.join("config.toml"), original).unwrap();
    let record = ActivationRecord {
        token: "sk-click-test-00000".into(),
        ..Default::default()
    };
    let result = cc_switch_lib::xinghuo::finish_use_app(
        &state,
        record,
        ApplyPlan::defaults(),
        ManagedApp::Codex,
    )
    .unwrap();
    assert_eq!(result.apps.len(), 1);
    assert!(result.apps[0].ok);
    let provider = ProviderService::list(&state, AppType::Codex)
        .unwrap()
        .shift_remove("xinghuo-codex")
        .unwrap();
    assert_eq!(provider.meta.unwrap().common_config_enabled, Some(true));
    assert!(codex_config().contains(SITE_URL));
    assert!(!has_provider(&state, AppType::Claude, "xinghuo-claude"));
    assert!(!has_provider(&state, AppType::OpenCode, "xinghuo-opencode"));
    assert!(
        cc_switch_lib::xinghuo::restore::restore_selected(&state, &[ManagedApp::Codex])
            .iter()
            .all(|app| app.ok)
    );
    let mut restored: toml::Value = codex_config().parse().unwrap();
    assert_eq!(
        restored["model_providers"]["custom"]["name"].as_str(),
        Some("OpenAI")
    );
    restored.as_table_mut().unwrap().remove("model_providers");
    assert_eq!(restored, original.parse::<toml::Value>().unwrap());
    let record = cc_switch_lib::xinghuo::active_record().unwrap();
    let result = cc_switch_lib::xinghuo::finish_use_app(
        &state,
        record,
        ApplyPlan::defaults(),
        ManagedApp::Codex,
    )
    .unwrap();
    assert!(result.apps[0].ok);
    assert!(codex_config().contains(SITE_URL));
    assert_eq!(
        ProviderService::list(&state, AppType::Codex).unwrap().len(),
        1
    );
    assert!(
        cc_switch_lib::xinghuo::restore::restore_selected(&state, &[ManagedApp::Codex])
            .iter()
            .all(|app| app.ok)
    );
    let mut restored: toml::Value = codex_config().parse().unwrap();
    assert_eq!(
        restored["model_providers"]["custom"]["name"].as_str(),
        Some("OpenAI")
    );
    restored.as_table_mut().unwrap().remove("model_providers");
    assert_eq!(restored, original.parse::<toml::Value>().unwrap());
}

#[test]
fn one_click_rejects_unavailable_tool_without_writing_configuration() {
    let _guard = test_mutex().lock().unwrap_or_else(|e| e.into_inner());
    reset_test_fs();
    let state = create_test_state().expect("create state");
    assert_eq!(
        cc_switch_lib::xinghuo::active_record().unwrap_err().code,
        "NOT_ACTIVATED"
    );
    let plan = ApplyPlan {
        apps: vec![ManagedApp::Claude],
        ..ApplyPlan::defaults()
    };
    let error = cc_switch_lib::xinghuo::finish_use_app(
        &state,
        ActivationRecord::default(),
        plan,
        ManagedApp::Codex,
    )
    .unwrap_err();
    assert_eq!(error.code, "APP_UNAVAILABLE");
    assert!(!has_provider(&state, AppType::Codex, "xinghuo-codex"));
}

#[test]
fn one_click_preserves_preferences_without_inheriting_a_different_endpoint() {
    let _guard = test_mutex().lock().unwrap_or_else(|e| e.into_inner());
    reset_test_fs();
    let state = create_test_state().unwrap();
    state.db.set_config_snippet("codex", Some("model = \"old-model\"\nmodel_provider = \"old\"\n\n[model_providers.old]\nbase_url = \"https://old.example/v1\"\n\n[tui]\nnotifications = false\n".into())).unwrap();
    let record = ActivationRecord {
        token: "sk-preference-test-00000".into(),
        ..Default::default()
    };
    let result = cc_switch_lib::xinghuo::finish_use_app(
        &state,
        record,
        ApplyPlan::defaults(),
        ManagedApp::Codex,
    )
    .unwrap();
    assert!(result.apps[0].ok);
    let config = codex_config();
    assert!(config.contains(SITE_URL));
    assert!(config.contains("notifications = false"));
    assert!(!config.contains("old.example"));
    assert!(!config.contains("old-model"));
}

#[test]
fn codex_invalid_writes_leave_config_and_login_unchanged() {
    let _guard = test_mutex().lock().unwrap_or_else(|e| e.into_inner());
    reset_test_fs();
    let home = ensure_test_home();
    let dir = home.join(".codex");
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("config.toml");
    let auth = dir.join("auth.json");
    let original = "model = \"gpt-5.5\"\n";
    std::fs::write(&config, original).unwrap();
    std::fs::write(&auth, br#"{"tokens":{"access_token":"fixture"}}"#).unwrap();
    let original_auth = std::fs::read(&auth).unwrap();
    for invalid in [
        "model_provider = \"missing\"",
        "[profiles.work]\nmodel_provider = \"missing\"",
        "model = [",
    ] {
        assert!(cc_switch_lib::write_codex_live_atomic(
            &json!({"OPENAI_API_KEY": "fixture-new"}),
            Some(invalid)
        )
        .is_err());
        assert_eq!(std::fs::read_to_string(&config).unwrap(), original);
        assert_eq!(std::fs::read(&auth).unwrap(), original_auth);
    }
}

#[test]
fn desktop_repeated_connection_checks_files_without_rewriting() {
    let _guard = test_mutex().lock().unwrap_or_else(|e| e.into_inner());
    reset_test_fs();
    ensure_test_home();
    let state = create_test_state().expect("create test state");
    tauri::async_runtime::block_on(async {
        let mut config = state.db.get_global_proxy_config().await.unwrap();
        config.listen_port = 0;
        state.db.update_global_proxy_config(config).await.unwrap();
    });
    let mut record = ActivationRecord {
        token: "sk-desktop-repeat-test".into(),
        ..Default::default()
    };
    let plan = ApplyPlan {
        apps: vec![ManagedApp::ClaudeDesktop],
        claude_desktop_models: vec!["claude-sonnet-5".into(), "claude-opus-5".into()],
        ..ApplyPlan::defaults()
    };
    let applied = apply_providers(&state, &record.token.clone(), &plan, false, &mut record);
    assert!(applied[0].ok, "{applied:?}");
    let snapshots: serde_json::Value =
        read_json_file(&ensure_test_home().join(".xinghuo-token/restore.json")).unwrap();
    let paths: Vec<std::path::PathBuf> = snapshots["claude-desktop"]["files"]
        .as_object()
        .unwrap()
        .keys()
        .map(std::path::PathBuf::from)
        .collect();
    let profile_index = paths
        .iter()
        .position(|path| {
            path.to_string_lossy().contains("configLibrary")
                && path.file_name().unwrap() != "meta.json"
        })
        .unwrap();
    let before: Vec<_> = paths
        .iter()
        .map(|path| {
            (
                std::fs::read(path).unwrap(),
                std::fs::metadata(path).unwrap().modified().unwrap(),
            )
        })
        .collect();
    let result = cc_switch_lib::xinghuo::desktop_already_connected(&state, &record).unwrap();
    assert!(result.already_connected);
    for (path, (content, modified)) in paths.iter().zip(&before) {
        assert_eq!(&std::fs::read(path).unwrap(), content);
        assert_eq!(
            std::fs::metadata(path).unwrap().modified().unwrap(),
            *modified
        );
    }
    // 配置漂移不能误报成功，恢复后也必须允许重新接入。
    let profile_path = &paths[profile_index];
    let mut profile: serde_json::Value = read_json_file(profile_path).unwrap();
    assert_eq!(profile["inferenceModels"].as_array().unwrap().len(), 2);
    profile["inferenceGatewayApiKey"] = json!("wrong-token");
    std::fs::write(profile_path, serde_json::to_vec(&profile).unwrap()).unwrap();
    assert!(cc_switch_lib::xinghuo::desktop_already_connected(&state, &record).is_none());
    std::fs::write(profile_path, &before[profile_index].0).unwrap();
    let mut wrong_key = record.clone();
    wrong_key.token = "sk-replaced-key".into();
    assert!(cc_switch_lib::xinghuo::desktop_already_connected(&state, &wrong_key).is_none());
    cc_switch_lib::xinghuo::restore::restore_selected(&state, &[ManagedApp::ClaudeDesktop]);
    assert!(cc_switch_lib::xinghuo::desktop_already_connected(&state, &record).is_none());
    tauri::async_runtime::block_on(state.proxy_service.stop()).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn cdk_exchange_is_device_free_and_idempotent() {
    use axum::{
        routing::{get, post},
        Json, Router,
    };
    use serde_json::Value;
    use std::sync::{Arc, Mutex};

    let _guard = test_mutex().lock().unwrap_or_else(|e| e.into_inner());
    reset_test_fs();
    let local = ensure_test_home().join(".xinghuo-token");
    let activations = Arc::new(Mutex::new(Vec::<Value>::new()));
    let activation_capture = activations.clone();
    let tokens = Arc::new(Mutex::new(Vec::<String>::new()));
    let status_capture = tokens.clone();
    let app = Router::new()
        .route(
            "/api/v1/plugin/activate",
            post(move |Json(body): Json<Value>| {
                let captured = activation_capture.clone();
                async move {
                    captured.lock().unwrap().push(body);
                    Json(json!({"ok": true, "token": "sk-shared-key-000000"}))
                }
            }),
        )
        .route(
            "/api/v1/plugin/status",
            get(move |headers: axum::http::HeaderMap| {
                let captured = status_capture.clone();
                async move {
                    captured.lock().unwrap().push(
                        headers["authorization"]
                            .to_str()
                            .unwrap()
                            .trim_start_matches("Bearer ")
                            .to_string(),
                    );
                    Json(json!({
                        "ok": true,
                        "valid": true,
                        "plan": "shared",
                        "quota": {"unit": "USD", "total": 100, "used": 11.5, "remaining": 88.5},
                        "balance_scope": "account"
                    }))
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    std::env::set_var("XINGHUO_SITE_URL", format!("http://{address}"));

    let first = cc_switch_lib::xinghuo::obtain_key("XH-TEST-SHARED-QUOTA")
        .await
        .unwrap();
    assert_eq!(first.record.token, "sk-shared-key-000000");
    assert_eq!(first.record.quota.unwrap().remaining, 88.5);
    assert_eq!(first.record.valid, Some(true));
    assert!(!local.join("device_id").exists());

    let second = cc_switch_lib::xinghuo::obtain_key("XH-TEST-SHARED-QUOTA")
        .await
        .unwrap();
    assert_eq!(second.record.token, first.record.token);
    assert!(second.renewal);

    let direct = cc_switch_lib::xinghuo::obtain_key("sk-shared-key-000000")
        .await
        .unwrap();
    assert_eq!(direct.record.source, "key");
    assert_eq!(direct.record.quota.unwrap().remaining, 88.5);
    assert_eq!(direct.record.valid, Some(true));

    let activations = activations.lock().unwrap();
    assert_eq!(activations.len(), 1);
    assert!(activations[0].get("device_id").is_none());
    assert!(activations[0].get("device_name").is_none());
    assert_eq!(activations[0]["cdk"], "XH-TEST-SHARED-QUOTA");
    assert_eq!(
        tokens.lock().unwrap().as_slice(),
        ["sk-shared-key-000000", "sk-shared-key-000000"]
    );

    drop(activations);
    std::env::remove_var("XINGHUO_SITE_URL");
    server.abort();
}
