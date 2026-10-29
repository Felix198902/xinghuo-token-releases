import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createTestQueryClient } from "../utils/testQueryClient";
import { DesktopConnectionDialog } from "@/components/activation/DesktopConnectionDialog";
import type { XinghuoDesktopRuntimeStatus } from "@/lib/api/xinghuo";

const { discover, configure } = vi.hoisted(() => ({
  discover: vi.fn(),
  configure: vi.fn(),
}));
vi.mock("@/lib/api/xinghuo", async () => ({
  ...(await vi.importActual<typeof import("@/lib/api/xinghuo")>(
    "@/lib/api/xinghuo",
  )),
  xinghuoApi: { discoverDesktopModels: discover, configureDesktop: configure },
}));

function panel(runtime?: XinghuoDesktopRuntimeStatus) {
  const onConfigured = vi.fn().mockResolvedValue(undefined);
  const onClose = vi.fn();
  render(
    <QueryClientProvider client={createTestQueryClient()}>
      <DesktopConnectionDialog
        view={{
          activated: true,
          apps: [],
          hasWallet: true,
          maskedKey: "sk-main…1234",
          desktopRuntime: runtime,
        }}
        onClose={onClose}
        onConfigured={onConfigured}
      />
    </QueryClientProvider>,
  );
  return { onConfigured, onClose };
}

describe("DesktopConnectionDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("discovers all API models and configures only Desktop", async () => {
    discover.mockResolvedValue([
      "deepseek-v4-pro",
      "hermes-3",
      "claude-opus-4-7",
      "claude-sonnet-5",
    ]);
    configure.mockResolvedValue({
      view: { activated: true },
      apps: [{ app: "claude-desktop", ok: true }],
    });
    const { onConfigured, onClose } = panel();
    expect(screen.queryByLabelText("上游协议")).not.toBeInTheDocument();
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "保存并接入" })).toBeDisabled();
    fireEvent.change(screen.getByLabelText("星火接口密钥"), {
      target: { value: " sk-user-desktop " },
    });
    fireEvent.click(screen.getByText("读取模型"));
    await waitFor(() =>
      expect(discover).toHaveBeenCalledWith("sk-user-desktop"),
    );
    await screen.findByText("deepseek-v4-pro");
    expect(screen.getByText("hermes-3")).toBeInTheDocument();
    expect(screen.getByText("claude-opus-4-7")).toBeInTheDocument();
    expect(screen.getByText("claude-sonnet-5")).toBeInTheDocument();
    fireEvent.click(screen.getByText("保存并接入"));
    await waitFor(() =>
      expect(configure).toHaveBeenCalledWith("sk-user-desktop", null),
    );
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(onConfigured).toHaveBeenCalled();
  });

  it("invalidates the previous catalog when the key changes", async () => {
    discover.mockResolvedValue(["claude-opus-4-7"]);
    panel();
    fireEvent.click(screen.getByText("读取模型"));
    await screen.findByText("claude-opus-4-7");
    fireEvent.change(screen.getByLabelText("星火接口密钥"), {
      target: { value: "sk-other-user" },
    });
    expect(screen.getByRole("button", { name: "保存并接入" })).toBeDisabled();
    expect(configure).not.toHaveBeenCalled();
  });

  it("keeps discovery errors visible without writing configuration", async () => {
    discover.mockRejectedValue({
      code: "HTTP_ERROR",
      status: 403,
      message: "private upstream detail",
    });
    panel();
    fireEvent.click(screen.getByText("读取模型"));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "密钥无效或没有访问权限",
    );
    expect(configure).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "保存并接入" })).toBeDisabled();
  });

  it("shows quota exhaustion when model discovery returns 429", async () => {
    discover.mockRejectedValue({
      code: "API_KEY_QUOTA_EXHAUSTED",
      status: 429,
      message: "API key 额度已用完",
    });
    panel();
    fireEvent.click(screen.getByText("读取模型"));
    expect(await screen.findByRole("alert")).toHaveTextContent("额度已用完");
    expect(screen.getByRole("button", { name: "保存并接入" })).toBeDisabled();
  });

  it("keeps the offline runtime remedy visible when workspace components are missing", async () => {
    const runtime = {
      helperInstalled: false,
      workspaceInstalled: false,
      offlineInstallerUrl:
        "https://claude.ai/api/desktop/win32/x64/offline/latest/redirect",
    };
    discover.mockResolvedValue(["claude-opus-5-5"]);
    configure.mockResolvedValue({
      view: { activated: true, desktopRuntime: runtime },
      apps: [{ app: "claude-desktop", ok: true }],
    });
    const { onConfigured, onClose } = panel(runtime);

    expect(screen.getByRole("alert")).toHaveTextContent(
      "Code/Cowork 可能无法启动",
    );
    fireEvent.click(screen.getByText("读取模型"));
    await screen.findByText("claude-opus-5-5");
    fireEvent.click(screen.getByText("保存并接入"));

    await waitFor(() => expect(onConfigured).toHaveBeenCalled());
    expect(onClose).not.toHaveBeenCalled();
    expect(
      screen.getByRole("button", { name: "下载官方离线完整版" }),
    ).toBeInTheDocument();
  });
});
