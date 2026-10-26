import { invoke } from "@tauri-apps/api/core";

/** 激活页负责配置的工具（与 Rust 端 ManagedApp 一致） */
export type XinghuoManagedApp =
  | "claude"
  | "claude-desktop"
  | "codex"
  | "opencode"
  | "gemini"
  | "grokbuild"
  | "openclaw"
  | "hermes"
  | "mcode"
  | "pi";

export interface XinghuoQuota {
  unit?: string;
  total: number;
  used: number;
  remaining: number;
}

/** 本机激活状态。密钥只以打码形式返回，完整密钥不进 WebView。 */
export interface XinghuoActivationView {
  activated: boolean;
  restoreAvailable?: boolean;
  maskedKey?: string;
  /** cdk：激活码换来的；key：直接粘贴的 sk- 密钥 */
  source?: string;
  plan?: string;
  expiresAt?: string | null;
  quota?: XinghuoQuota;
  /** 额度口径：密钥消费上限、账户余额或未绑定套餐余额 */
  balanceScope?: "key_cap" | "account" | "shadow";
  /** 最近一次查询到的密钥有效性，未查询时缺省 */
  valid?: boolean;
  apps: XinghuoManagedApp[];
  unitLabel?: string;
  contact?: string;
  shopUrl?: string;
  activatedAt?: number;
  hasWallet: boolean;
  desktopConnectionConfigured?: boolean;
  desktopMaskedKey?: string;
  desktopApiFormat?: DesktopApiFormat;
  desktopModel?: string;
}

export type DesktopApiFormat = "anthropic" | "openai_chat" | "openai_responses";

export interface XinghuoAppliedApp {
  app: XinghuoManagedApp;
  ok: boolean;
  error?: string;
  warnings?: string[];
}

export interface XinghuoActivateResult {
  view: XinghuoActivationView;
  apps: XinghuoAppliedApp[];
  /** 续费：拿回的是本机已有的同一把密钥，各工具配置未改动 */
  renewed: boolean;
  alreadyConnected?: boolean;
}

export interface XinghuoDeactivateResult {
  view: XinghuoActivationView;
  apps: XinghuoAppliedApp[];
}

/** 命令失败时的错误：code 决定顾客看到的文案，status 用来拼 XH-4<状态码> */
export interface XinghuoError {
  code: string;
  status?: number;
  message: string;
}

/** 星火供应商的用量模板类型（与 Rust 端 USAGE_TEMPLATE_TYPE 一致） */
export const XINGHUO_USAGE_TEMPLATE = "xinghuo";

/** 由激活页建立、余额走星火原生查询的供应商 */
export function isXinghuoProvider(provider: {
  meta?: { usage_script?: { templateType?: string } };
}): boolean {
  return provider.meta?.usage_script?.templateType === XINGHUO_USAGE_TEMPLATE;
}

export function isXinghuoError(value: unknown): value is XinghuoError {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { code?: unknown }).code === "string"
  );
}

export const xinghuoApi = {
  getActivation(): Promise<XinghuoActivationView> {
    return invoke("get_xinghuo_activation");
  },

  activate(input: string): Promise<XinghuoActivateResult> {
    return invoke("activate_xinghuo", { input });
  },

  useApp(app: XinghuoManagedApp): Promise<XinghuoActivateResult> {
    return invoke("use_xinghuo_app", { app });
  },

  refreshStatus(): Promise<XinghuoActivationView> {
    return invoke("refresh_xinghuo_status");
  },

  discoverDesktopModels(input: string): Promise<string[]> {
    return invoke("discover_xinghuo_desktop_models", { input });
  },

  configureDesktop(
    input: string,
    model: string | null,
  ): Promise<XinghuoActivateResult> {
    return invoke("configure_xinghuo_desktop", { input, model });
  },

  restore(app?: XinghuoManagedApp): Promise<XinghuoDeactivateResult> {
    return invoke("restore_xinghuo_config", { app });
  },

  deactivate(): Promise<XinghuoDeactivateResult> {
    return invoke("deactivate_xinghuo");
  },
};
