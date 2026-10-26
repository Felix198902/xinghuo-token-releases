import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { XinghuoActivationView } from "@/lib/api/xinghuo";
import { isXinghuoProvider } from "@/lib/api/xinghuo";

const { openExternal } = vi.hoisted(() => ({ openExternal: vi.fn() }));

vi.mock("@/lib/api", () => ({
  settingsApi: { openExternal },
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

import {
  RenewalBanner,
  renewalReason,
  isKeyQuotaExhausted,
} from "@/components/activation/RenewalBanner";

const base: XinghuoActivationView = {
  activated: true,
  maskedKey: "sk-abc…wxyz",
  apps: ["claude"],
  hasWallet: true,
  valid: true,
  quota: { total: 100, used: 20, remaining: 80 },
};

describe("renewalReason", () => {
  it("ignores inactive or unchecked states", () => {
    expect(renewalReason(undefined)).toBeNull();
    expect(
      renewalReason({ activated: false, apps: [], hasWallet: false }),
    ).toBeNull();
    expect(renewalReason({ ...base, valid: undefined })).toBeNull();
  });

  it("trusts server validity over a zero balance", () => {
    // 订阅计费分组：余额可能为 0，但服务端判定仍然有效
    expect(
      renewalReason({
        ...base,
        valid: true,
        quota: { total: 0, used: 0, remaining: 0 },
      }),
    ).toBeNull();
  });

  it("区分密钥消费上限与账户余额不足", () => {
    expect(
      isKeyQuotaExhausted({
        ...base,
        balanceScope: "key_cap",
        quota: { total: 5, used: 5.57295725, remaining: 0 },
      }),
    ).toBe(true);
    expect(
      isKeyQuotaExhausted({
        ...base,
        balanceScope: "key_cap",
        quota: { total: 5, used: 2, remaining: 0 },
      }),
    ).toBe(false);
    expect(
      isKeyQuotaExhausted({
        ...base,
        balanceScope: "account",
        quota: { total: 0, used: 0, remaining: 0 },
      }),
    ).toBe(false);
  });

  it("distinguishes an exhausted balance from an expired or disabled key", () => {
    expect(
      renewalReason({
        ...base,
        valid: false,
        quota: { total: 100, used: 100, remaining: 0 },
      }),
    ).toBe("exhausted");
    expect(renewalReason({ ...base, valid: false })).toBe("invalid");
  });
});

describe("RenewalBanner", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders nothing while the key is fine", () => {
    const { container } = render(
      <RenewalBanner view={base} onOpenActivation={vi.fn()} />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("offers renewal, code entry and dismissal when the balance is used up", () => {
    const onOpenActivation = vi.fn();
    render(
      <RenewalBanner
        view={{
          ...base,
          valid: false,
          quota: { total: 100, used: 100, remaining: 0 },
          shopUrl: "https://shop.example/goods/1",
        }}
        onOpenActivation={onOpenActivation}
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent(
      "activation.banner.exhausted",
    );

    fireEvent.click(screen.getByText("activation.banner.renew"));
    expect(openExternal).toHaveBeenCalledWith("https://shop.example/goods/1");

    fireEvent.click(screen.getByText("activation.banner.enterCode"));
    expect(onOpenActivation).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByLabelText("activation.banner.dismiss"));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});

describe("isXinghuoProvider", () => {
  it("matches only providers using the xinghuo usage template", () => {
    expect(
      isXinghuoProvider({
        meta: { usage_script: { templateType: "xinghuo" } },
      }),
    ).toBe(true);
    expect(
      isXinghuoProvider({
        meta: { usage_script: { templateType: "general" } },
      }),
    ).toBe(false);
    expect(isXinghuoProvider({})).toBe(false);
  });
});
