import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { XinghuoActivationView } from "@/lib/api/xinghuo";
import { createTestQueryClient } from "../utils/testQueryClient";

const {
  getActivation,
  activate,
  useApp,
  refreshStatus,
  deactivate,
  restore,
  updateTrayMenu,
  openExternal,
  toastSuccess,
  toastError,
  toastWarning,
} = vi.hoisted(() => ({
  getActivation: vi.fn(),
  activate: vi.fn(),
  useApp: vi.fn(),
  refreshStatus: vi.fn(),
  deactivate: vi.fn(),
  restore: vi.fn(),
  updateTrayMenu: vi.fn(),
  openExternal: vi.fn(),
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
  toastWarning: vi.fn(),
}));

vi.mock("@/lib/api/xinghuo", async () => {
  const actual =
    await vi.importActual<typeof import("@/lib/api/xinghuo")>(
      "@/lib/api/xinghuo",
    );
  return {
    ...actual,
    xinghuoApi: {
      getActivation,
      activate,
      useApp,
      refreshStatus,
      deactivate,
      restore,
    },
  };
});

vi.mock("@/lib/api", () => ({
  providersApi: { updateTrayMenu },
  settingsApi: { openExternal },
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown>) =>
      options && "apps" in options ? `${key}:${String(options.apps)}` : key,
  }),
}));

vi.mock("sonner", () => ({
  toast: {
    success: toastSuccess,
    error: toastError,
    warning: toastWarning,
    info: vi.fn(),
  },
}));

vi.mock("@/components/ConfirmDialog", () => ({
  ConfirmDialog: ({
    isOpen,
    confirmText,
    onConfirm,
  }: {
    isOpen: boolean;
    confirmText: string;
    onConfirm: () => void;
  }) =>
    isOpen ? (
      <button onClick={() => onConfirm()}>{`confirm:${confirmText}`}</button>
    ) : null,
}));

import { ActivationPanel } from "@/components/activation/ActivationPanel";

const inactiveView: XinghuoActivationView = {
  activated: false,
  apps: [],
  hasWallet: false,
};

const activeView: XinghuoActivationView = {
  activated: true,
  maskedKey: "sk-abc…wxyz",
  source: "cdk",
  plan: "pro",
  expiresAt: "2026-12-31T15:59:59Z",
  quota: { unit: "USD", total: 100, used: 11.5, remaining: 88.5 },
  valid: true,
  apps: ["claude", "codex", "opencode"],
  hasWallet: true,
  restoreAvailable: true,
};

function renderPanel() {
  const client = createTestQueryClient();
  return render(
    <QueryClientProvider client={client}>
      <ActivationPanel />
    </QueryClientProvider>,
  );
}

describe("ActivationPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    updateTrayMenu.mockResolvedValue(true);
    refreshStatus.mockResolvedValue(activeView);
  });

  it("密钥消费触顶时显示上限与对应原因", async () => {
    const cappedView: XinghuoActivationView = {
      ...activeView,
      valid: false,
      balanceScope: "key_cap",
      quota: { total: 5, used: 5.57295725, remaining: 0 },
    };
    getActivation.mockResolvedValue(cappedView);
    refreshStatus.mockResolvedValue(cappedView);
    renderPanel();
    await screen.findByText("activation.keyExhausted");
    expect(screen.getByText("activation.keyQuotaLimit")).toBeInTheDocument();
    expect(screen.getByText("activation.keyExhaustedHint")).toBeInTheDocument();
    expect(screen.getByText("¥5.00")).toBeInTheDocument();
    expect(screen.getByText("¥5.57")).toBeInTheDocument();
    expect(
      screen.queryByText("activation.exhaustedHint"),
    ).not.toBeInTheDocument();
  });

  it("首次进入不弹出引导，点击一键接入打开输入弹窗且没有跳过", async () => {
    getActivation.mockResolvedValue(inactiveView);
    renderPanel();
    await screen.findByText("Codex");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    fireEvent.click(screen.getByText("一键接入"));
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    expect(
      screen.getByLabelText("activation.inputPlaceholder"),
    ).toBeInTheDocument();
    expect(screen.queryByText("跳过，进入主界面")).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("activation.inputPlaceholder"), {
      target: { value: "XH-CANCELLED" },
    });
    fireEvent.click(screen.getByText("取消"));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    fireEvent.click(screen.getByText("一键接入"));
    expect(screen.getByLabelText("activation.inputPlaceholder")).toHaveValue(
      "",
    );
    expect(activate).not.toHaveBeenCalled();
    expect(useApp).not.toHaveBeenCalled();
  });

  it("激活失败保留输入弹窗，主动接入不允许跳过", async () => {
    getActivation.mockResolvedValue(inactiveView);
    activate.mockRejectedValue({ code: "CDK_NOT_FOUND", status: 403 });
    renderPanel();
    fireEvent.click(await screen.findByText("一键接入"));
    fireEvent.change(screen.getByLabelText("activation.inputPlaceholder"), {
      target: { value: "XH-TEST" },
    });
    fireEvent.click(screen.getByText("activation.activate"));
    await screen.findByRole("alert");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.queryByText("跳过，进入主界面")).not.toBeInTheDocument();
  });

  it("QQ 与微信二维码可同时展开，各自点击独立收起", async () => {
    getActivation.mockResolvedValue(inactiveView);
    renderPanel();
    await screen.findByText("Codex");
    expect(
      screen.queryByRole("img", { name: /加好友二维码/ }),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "QQ 169106354" }));
    expect(
      screen.getByRole("img", { name: "QQ 169106354 加好友二维码" }),
    ).toBeInTheDocument();
    openExternal.mockResolvedValue(undefined);
    fireEvent.click(screen.getByText("打开 QQ 聊天"));
    expect(openExternal).toHaveBeenCalledWith(
      "https://wpa.qq.com/msgrd?v=3&uin=169106354&site=qq&menu=yes",
    );
    fireEvent.click(screen.getByRole("button", { name: "微信 US666377" }));
    expect(
      screen.getByRole("img", { name: "QQ 169106354 加好友二维码" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("img", { name: "微信 US666377 加好友二维码" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "微信 US666377" }));
    expect(
      screen.queryByRole("img", { name: "微信 US666377 加好友二维码" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("img", { name: "QQ 169106354 加好友二维码" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "QQ 169106354" }));
    expect(
      screen.queryByRole("img", { name: /加好友二维码/ }),
    ).not.toBeInTheDocument();
  });

  it("服务端只下发 QQ 时仍保留微信账号和二维码", async () => {
    const qqOnlyView = { ...activeView, contact: "169106354" };
    getActivation.mockResolvedValue(qqOnlyView);
    refreshStatus.mockResolvedValue(qqOnlyView);
    renderPanel();
    await screen.findByText("activation.activated");
    await waitFor(() => expect(refreshStatus).toHaveBeenCalled());
    expect(
      screen.getByRole("button", { name: "QQ 169106354" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "微信 US666377" }));
    expect(
      screen.getByRole("img", { name: "微信 US666377 加好友二维码" }),
    ).toBeInTheDocument();
  });

  it("asks for input before calling the backend", async () => {
    getActivation.mockResolvedValue(inactiveView);
    renderPanel();
    fireEvent.click(await screen.findByText("Codex"));
    fireEvent.click(screen.getByText("一键接入"));

    fireEvent.click(await screen.findByText("activation.activate"));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "activation.errors.inputEmpty",
    );
    expect(activate).not.toHaveBeenCalled();
  });

  it("activates with the trimmed input and reports configured tools", async () => {
    getActivation.mockResolvedValue(inactiveView);
    activate.mockResolvedValue({
      view: activeView,
      apps: [
        { app: "claude", ok: true },
        { app: "codex", ok: true },
        { app: "opencode", ok: false, error: "write failed" },
      ],
      renewed: false,
    });
    renderPanel();
    fireEvent.click(await screen.findByText("Codex"));
    fireEvent.click(screen.getByText("一键接入"));

    fireEvent.change(
      await screen.findByLabelText("activation.inputPlaceholder"),
      { target: { value: "  XH-ABCDE-FGHJK-MNPQR  " } },
    );
    fireEvent.click(screen.getByText("activation.activate"));

    await waitFor(() =>
      expect(activate).toHaveBeenCalledWith("XH-ABCDE-FGHJK-MNPQR"),
    );
    await waitFor(() =>
      expect(toastSuccess).toHaveBeenCalledWith(
        "activation.success:activation.apps.claude、activation.apps.codex",
        expect.anything(),
      ),
    );
    expect(toastError).toHaveBeenCalledWith(
      "activation.partialFailure",
      expect.anything(),
    );
    expect(updateTrayMenu).toHaveBeenCalled();
    expect(await screen.findByText("sk-abc…wxyz")).toBeInTheDocument();
  });

  it("warns when the key has no tools to configure", async () => {
    getActivation.mockResolvedValue(inactiveView);
    activate.mockResolvedValue({
      view: { ...activeView, apps: [] },
      apps: [],
      renewed: false,
    });
    renderPanel();
    fireEvent.click(await screen.findByText("Codex"));
    fireEvent.click(screen.getByText("一键接入"));

    fireEvent.change(
      await screen.findByLabelText("activation.inputPlaceholder"),
      { target: { value: "sk-key-without-tools" } },
    );
    fireEvent.click(screen.getByText("activation.activate"));

    await waitFor(() =>
      expect(toastWarning).toHaveBeenCalledWith(
        "activation.successNoApps",
        expect.anything(),
      ),
    );
    expect(toastSuccess).not.toHaveBeenCalled();
  });

  it("shows the mapped message when activation fails", async () => {
    getActivation.mockResolvedValue(inactiveView);
    activate.mockRejectedValue({
      code: "CDK_NOT_FOUND",
      status: 404,
      message: "激活码不存在",
    });
    renderPanel();
    fireEvent.click(await screen.findByText("Codex"));
    fireEvent.click(screen.getByText("一键接入"));

    fireEvent.change(
      await screen.findByLabelText("activation.inputPlaceholder"),
      { target: { value: "XH-ABCDE-FGHJK-MNPQR" } },
    );
    fireEvent.click(screen.getByText("activation.activate"));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "activation.errors.cdkNotFound",
    );
  });

  it("tells an exhausted balance apart from an invalid key", async () => {
    const exhausted: XinghuoActivationView = {
      ...activeView,
      valid: false,
      quota: { unit: "USD", total: 100, used: 100, remaining: 0 },
    };
    getActivation.mockResolvedValue(exhausted);
    refreshStatus.mockResolvedValue(exhausted);
    renderPanel();

    expect(await screen.findByText("activation.exhausted")).toBeInTheDocument();
    expect(screen.getByText("activation.exhaustedHint")).toBeInTheDocument();
    expect(screen.queryByText("activation.invalid")).not.toBeInTheDocument();
  });

  it("opens Desktop key setup instead of reusing an exhausted activation key", async () => {
    const exhausted: XinghuoActivationView = {
      ...activeView,
      valid: false,
      quota: { unit: "USD", total: 100, used: 100, remaining: 0 },
    };
    getActivation.mockResolvedValue(exhausted);
    refreshStatus.mockResolvedValue(exhausted);
    renderPanel();
    await screen.findByText("sk-abc…wxyz");
    fireEvent.click(await screen.findByText("Claude Desktop"));
    fireEvent.click(screen.getByText("一键接入"));
    expect(await screen.findByLabelText("星火接口密钥")).toBeInTheDocument();
    expect(
      screen.getByText(/输入新密钥仅用于 Claude Desktop/),
    ).toBeInTheDocument();
    expect(useApp).not.toHaveBeenCalled();
  });

  it("shows status, refreshes once and supports deactivation", async () => {
    getActivation.mockResolvedValue(activeView);
    deactivate.mockResolvedValue({
      view: inactiveView,
      apps: [
        { app: "claude", ok: true },
        { app: "codex", ok: true },
        { app: "opencode", ok: true },
      ],
    });
    renderPanel();
    await screen.findByText("sk-abc…wxyz");

    expect(await screen.findByText("sk-abc…wxyz")).toBeInTheDocument();
    expect(screen.getByText("¥88.50")).toBeInTheDocument();
    expect(
      screen.queryByLabelText("activation.renewPlaceholder"),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByText("续费 / 更换激活码"));
    expect(screen.getByText("activation.renew")).toBeInTheDocument();
    await waitFor(() => expect(refreshStatus).toHaveBeenCalledTimes(1));

    fireEvent.click(screen.getByText("activation.deactivate"));
    fireEvent.click(await screen.findByText("confirm:activation.deactivate"));

    await waitFor(() => expect(deactivate).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(toastSuccess).toHaveBeenCalledWith(
        "activation.deactivateSuccess",
        expect.anything(),
      ),
    );
    expect(await screen.findByText("Codex")).toBeInTheDocument();
    expect(
      screen.queryByLabelText("activation.inputPlaceholder"),
    ).not.toBeInTheDocument();
  });
  it("一键使用直接调用后端，不显示供应商编辑或未支持的工具", async () => {
    getActivation.mockResolvedValue(activeView);
    useApp.mockResolvedValue({
      view: activeView,
      apps: [{ app: "codex", ok: true }],
      renewed: false,
    });
    renderPanel();
    await screen.findByText("sk-abc…wxyz");
    const button = await screen.findByText("Codex");
    expect(screen.queryByText("Gemini")).not.toBeInTheDocument();
    expect(screen.getByText("Claude Desktop")).toBeInTheDocument();
    expect(screen.queryByText("添加供应商")).not.toBeInTheDocument();
    expect(
      screen.queryByLabelText("activation.renewPlaceholder"),
    ).not.toBeInTheDocument();
    fireEvent.click(button);
    expect(useApp).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText("一键接入"));
    await waitFor(() => expect(useApp).toHaveBeenCalledWith("codex"));
    await waitFor(() =>
      expect(toastSuccess).toHaveBeenCalledWith(
        "已连接上 【ChatGPT】 官方服务器，请放心使用",
      ),
    );
    expect(activate).not.toHaveBeenCalled();
  });

  it("配置失败不能提示接入成功", async () => {
    getActivation.mockResolvedValue(activeView);
    useApp.mockResolvedValue({
      view: activeView,
      apps: [{ app: "codex", ok: false, error: "write failed" }],
      renewed: false,
    });
    renderPanel();
    await screen.findByText("sk-abc…wxyz");
    fireEvent.click(await screen.findByText("Codex"));
    fireEvent.click(screen.getByText("一键接入"));
    await waitFor(() =>
      expect(toastError).toHaveBeenCalledWith("activation.partialFailure"),
    );
    expect(toastSuccess).not.toHaveBeenCalled();
  });

  it("桌面版一键接入并提示已连接官方服务器", async () => {
    getActivation.mockResolvedValue(activeView);
    useApp.mockResolvedValue({
      view: { ...activeView, apps: [...activeView.apps, "claude-desktop"] },
      apps: [{ app: "claude-desktop", ok: true }],
      renewed: false,
    });
    renderPanel();
    await screen.findByText("sk-abc…wxyz");
    fireEvent.click(await screen.findByText("Claude Desktop"));
    expect(useApp).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText("一键接入"));
    await waitFor(() => expect(useApp).toHaveBeenCalledWith("claude-desktop"));
    await waitFor(() =>
      expect(toastSuccess).toHaveBeenCalledWith(
        "已连接上 【Claude Desktop】 官方服务器，请放心使用",
      ),
    );
  });

  it("桌面配置已核实接入时只提示成功，不要求重启", async () => {
    getActivation.mockResolvedValue(activeView);
    useApp.mockResolvedValue({
      view: activeView,
      apps: [{ app: "claude-desktop", ok: true }],
      renewed: false,
      alreadyConnected: true,
    });
    renderPanel();
    await screen.findByText("sk-abc…wxyz");
    fireEvent.click(screen.getByText("Claude Desktop"));
    fireEvent.click(screen.getByText("一键接入"));
    await waitFor(() =>
      expect(toastSuccess).toHaveBeenCalledWith("您已成功接入"),
    );
    expect(updateTrayMenu).not.toHaveBeenCalled();
    expect(toastError).not.toHaveBeenCalled();
  });

  it("接入过程中禁用其他写入，重复点击只提交一次", async () => {
    getActivation.mockResolvedValue(activeView);
    let finish!: (result: unknown) => void;
    useApp.mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    renderPanel();
    await screen.findByText("sk-abc…wxyz");
    const button = await screen.findByText("Codex");
    fireEvent.click(button);
    fireEvent.click(screen.getByText("一键接入"));
    fireEvent.click(screen.getByText("一键接入"));
    await waitFor(() => expect(useApp).toHaveBeenCalledTimes(1));
    expect(screen.getByText("一键还原")).toBeDisabled();
    expect(screen.getByText("Claude").closest("button")).toBeDisabled();
    finish({
      view: activeView,
      apps: [{ app: "codex", ok: true }],
      renewed: false,
    });
    await waitFor(() => expect(button).not.toBeDisabled());
  });

  it("一键还原时保留激活状态，不调用退出激活", async () => {
    getActivation.mockResolvedValue(activeView);
    restore.mockResolvedValue({
      view: activeView,
      apps: [{ app: "codex", ok: true }],
    });
    renderPanel();
    await screen.findByText("sk-abc…wxyz");
    fireEvent.click(await screen.findByText("Claude"));
    fireEvent.click(screen.getByText("一键还原"));
    await waitFor(() => expect(restore).toHaveBeenCalledWith("claude"));
    await waitFor(() =>
      expect(toastSuccess).toHaveBeenCalledWith(
        "已恢复切换前配置，重启对应工具后生效",
      ),
    );
    expect(deactivate).not.toHaveBeenCalled();
    expect(screen.getByText("sk-abc…wxyz")).toBeInTheDocument();
  });

  it("更多平台默认收起，展开后可一键接入 Gemini 并提示官方服务器", async () => {
    getActivation.mockResolvedValue(activeView);
    useApp.mockResolvedValue({
      view: { ...activeView, apps: [...activeView.apps, "gemini" as const] },
      apps: [{ app: "gemini", ok: true }],
      renewed: false,
    });
    renderPanel();
    await screen.findByText("sk-abc…wxyz");
    expect(screen.queryByText("Gemini")).not.toBeInTheDocument();
    fireEvent.click(screen.getByText("更多平台"));
    const gemini = await screen.findByText("Gemini");
    expect(screen.getByText("Grok Build")).toBeInTheDocument();
    expect(screen.getByText("OpenClaw")).toBeInTheDocument();
    expect(screen.getByText("Hermes")).toBeInTheDocument();
    expect(screen.getByText("MiniMax Code")).toBeInTheDocument();
    // Pi 图标 SVG 自带 <title>Pi</title>，会多匹配一个元素
    expect(screen.getAllByText("Pi").length).toBeGreaterThan(0);
    fireEvent.click(gemini);
    expect(useApp).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText("一键接入"));
    await waitFor(() => expect(useApp).toHaveBeenCalledWith("gemini"));
    await waitFor(() =>
      expect(toastSuccess).toHaveBeenCalledWith(
        "已连接上 【Gemini】 官方服务器，请放心使用",
      ),
    );
    fireEvent.click(screen.getByText("更多平台"));
    expect(screen.queryByText("Gemini")).not.toBeInTheDocument();
  });

  it("更多平台的官方服务器映射：Grok Build 提示 Grok，MiniMax Code 提示 MiniMax", async () => {
    getActivation.mockResolvedValue(activeView);
    useApp.mockImplementation((app: string) =>
      Promise.resolve({
        view: activeView,
        apps: [{ app, ok: true }],
        renewed: false,
      }),
    );
    renderPanel();
    await screen.findByText("sk-abc…wxyz");
    fireEvent.click(screen.getByText("更多平台"));
    fireEvent.click(await screen.findByText("Grok Build"));
    fireEvent.click(screen.getByText("一键接入"));
    await waitFor(() => expect(useApp).toHaveBeenCalledWith("grokbuild"));
    await waitFor(() =>
      expect(toastSuccess).toHaveBeenCalledWith(
        "已连接上 【Grok】 官方服务器，请放心使用",
      ),
    );
    fireEvent.click(screen.getByText("MiniMax Code"));
    fireEvent.click(screen.getByText("一键接入"));
    await waitFor(() => expect(useApp).toHaveBeenCalledWith("mcode"));
    await waitFor(() =>
      expect(toastSuccess).toHaveBeenCalledWith(
        "已连接上 【MiniMax】 官方服务器，请放心使用",
      ),
    );
  });
});
