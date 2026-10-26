/**
 * 品牌与站点常量：客户端里指向星火服务的地址、商店和联系方式都从这里取。
 *
 * Rust 端同名常量在 src-tauri/src/xinghuo/mod.rs（SITE_URL / PROVIDER_NAME /
 * DEFAULT_MODEL），改这里时一起改。服务端的 client-config 下发了联系方式和
 * 商店地址时，以服务端为准。
 */
export const BRAND = {
  /** 应用名 */
  name: "星火无限token",
  /** 各工具里星火供应商的显示名 */
  providerName: "星火无限token",
  /** 站点根地址（Claude Code 直接使用，不带 /v1） */
  site: "https://ai.wfpromax.xyz",
  /** OpenAI 兼容接口地址（Codex / OpenCode 使用） */
  apiBase: "https://ai.wfpromax.xyz/v1",
  /** 网页端「API 密钥」页 */
  keysUrl: "https://ai.wfpromax.xyz/keys",
  /** 发卡商店（购买激活码） */
  shopUrl: "https://shop.wfpromax.xyz",
  /** 系统管理员联系方式 */
  contact: "QQ 169106354 / 微信 US666377",
  /** 供应商图标名（src/icons/extracted） */
  icon: "xinghuo",
  /** 服务端未下发配置时的默认模型 */
  defaultModel: "gpt-5.5",
  /** 服务端未下发时的余额单位 */
  unitLabel: "¥",
} as const;
