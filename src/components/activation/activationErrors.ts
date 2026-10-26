import type { TFunction } from "i18next";
import { isXinghuoError } from "@/lib/api/xinghuo";

/** 有专门文案的错误码 → i18n key */
const KNOWN_CODES: Record<string, string> = {
  NOT_ACTIVATED: "activation.errors.notActivated",
  APP_UNAVAILABLE: "activation.errors.appUnavailable",
  INPUT_EMPTY: "activation.errors.inputEmpty",
  CDK_REQUIRED: "activation.errors.inputEmpty",
  CDK_NOT_FOUND: "activation.errors.cdkNotFound",
  CDK_EXPIRED: "activation.errors.cdkExpired",
  CDK_DISABLED: "activation.errors.cdkDisabled",
  DEVICE_LIMIT: "activation.errors.deviceLimit",
  CDK_IS_REDEEM_CODE: "activation.errors.redeemCode",
  CDK_TOKEN_REQUIRED: "activation.errors.tokenRequired",
  CDK_CLAIMED: "activation.errors.cdkClaimed",
  RATE_LIMITED: "activation.errors.rateLimited",
  DEVICE_ID_INVALID: "activation.errors.deviceIdInvalid",
  CDK_RETRY: "activation.errors.retry",
  KEY_INVALID: "activation.errors.keyInvalid",
};

/**
 * 把激活相关的错误转成顾客看到的一句话：只写现象、错误码和管理员联系方式，
 * 服务端原文不上屏。网络问题是 XH-4001，服务端出错是 XH-4<状态码>，
 * 与 xinghuo-connect 共用一张错误码表。
 */
export function describeActivationError(
  error: unknown,
  t: TFunction,
  contact: string,
): string {
  if (!isXinghuoError(error)) {
    return t("activation.errors.withCode", {
      phenomenon: t("activation.errors.failed"),
      code: "XH-4000",
      contact,
    });
  }

  const known = KNOWN_CODES[error.code];
  if (known) {
    return t(known, { contact });
  }

  if (error.code === "NETWORK") {
    return t("activation.errors.withCode", {
      phenomenon:
        error.message === "timeout"
          ? t("activation.errors.timeout")
          : t("activation.errors.unreachable"),
      code: "XH-4001",
      contact,
    });
  }

  if (error.code === "HTTP_ERROR" || error.code === "BAD_RESPONSE") {
    const status =
      error.status && error.status >= 100 && error.status <= 999
        ? String(error.status)
        : "000";
    return t("activation.errors.withCode", {
      phenomenon: t("activation.errors.serviceUnavailable"),
      code: `XH-4${status}`,
      contact,
    });
  }

  return t("activation.errors.withCode", {
    phenomenon: t("activation.errors.failed"),
    code: error.code,
    contact,
  });
}
