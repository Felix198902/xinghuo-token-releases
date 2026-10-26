import { describe, expect, it } from "vitest";
import type { TFunction } from "i18next";
import { describeActivationError } from "@/components/activation/activationErrors";

const t = ((key: string, options?: Record<string, unknown>) =>
  options ? `${key}|${JSON.stringify(options)}` : key) as unknown as TFunction;

const CONTACT = "QQ 1 / 微信 2";

describe("describeActivationError", () => {
  it("maps known server codes to dedicated messages", () => {
    expect(
      describeActivationError(
        {
          code: "DEVICE_LIMIT",
          status: 403,
          message: "该激活码绑定设备数已达上限",
        },
        t,
        CONTACT,
      ),
    ).toBe(
      `activation.errors.deviceLimit|${JSON.stringify({ contact: CONTACT })}`,
    );
    expect(
      describeActivationError(
        { code: "CDK_TOKEN_REQUIRED", status: 403, message: "x" },
        t,
        CONTACT,
      ),
    ).toContain("activation.errors.tokenRequired");
  });

  it("uses XH-4001 for network failures and tells timeouts apart", () => {
    const timeout = describeActivationError(
      { code: "NETWORK", message: "timeout" },
      t,
      CONTACT,
    );
    expect(timeout).toContain('"code":"XH-4001"');
    expect(timeout).toContain('"phenomenon":"activation.errors.timeout"');

    const unreachable = describeActivationError(
      { code: "NETWORK", message: "unreachable" },
      t,
      CONTACT,
    );
    expect(unreachable).toContain(
      '"phenomenon":"activation.errors.unreachable"',
    );
  });

  it("uses XH-4<status> for server errors without leaking the server text", () => {
    const text = describeActivationError(
      { code: "HTTP_ERROR", status: 502, message: "<html>Bad Gateway</html>" },
      t,
      CONTACT,
    );
    expect(text).toContain('"code":"XH-4502"');
    expect(text).toContain(CONTACT);
    expect(text).not.toContain("Bad Gateway");
  });

  it("falls back to the raw code plus contact for unknown codes", () => {
    const text = describeActivationError(
      { code: "STORAGE", message: "permission denied" },
      t,
      CONTACT,
    );
    expect(text).toContain('"code":"STORAGE"');
    expect(text).not.toContain("permission denied");
  });

  it("handles non-structured errors", () => {
    expect(describeActivationError(new Error("boom"), t, CONTACT)).toContain(
      '"code":"XH-4000"',
    );
  });
});
