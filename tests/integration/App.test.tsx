import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { http, HttpResponse } from "msw";
import { server } from "../msw/server";
import { emitTauriEvent } from "../msw/tauriMocks";
import { createTestQueryClient } from "../utils/testQueryClient";

const mocks = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
  checkUpdates: vi.fn(),
  openDiscovery: vi.fn(),
}));
vi.mock("sonner", () => ({
  toast: { success: mocks.success, error: mocks.error, warning: vi.fn() },
}));
vi.mock("@/components/settings/SettingsPage", () => ({
  SettingsPage: ({
    onOpenChange,
  }: {
    onOpenChange: (open: boolean) => void;
  }) => <button onClick={() => onOpenChange(false)}>close-settings</button>,
}));
vi.mock("@/components/skills/UnifiedSkillsPanel", async () => {
  const React = await import("react");
  return {
    default: React.forwardRef(
      (
        {
          onCheckUpdatesStateChange,
        }: {
          onCheckUpdatesStateChange?: (state: {
            isChecking: boolean;
            hasSkills: boolean;
          }) => void;
        },
        ref,
      ) => {
        React.useEffect(() => {
          onCheckUpdatesStateChange?.({ isChecking: false, hasSkills: true });
        }, [onCheckUpdatesStateChange]);
        React.useImperativeHandle(ref, () => ({
          checkUpdates: mocks.checkUpdates,
          openDiscovery: mocks.openDiscovery,
        }));
        return <div data-testid="skills-panel" />;
      },
    ),
  };
});
vi.mock("@/components/sessions/SessionManagerPage", () => ({
  SessionManagerPage: ({ appId }: { appId: string }) => (
    <div data-testid="sessions-app">{appId}</div>
  ),
}));

import App from "@/App";
import { ThemeProvider } from "@/components/theme-provider";
const active = {
  activated: true,
  maskedKey: "sk-test…0000",
  apps: ["claude", "codex", "opencode"],
  hasWallet: true,
  quota: { total: 100, used: 10, remaining: 90 },
};
function renderApp() {
  return render(
    <QueryClientProvider client={createTestQueryClient()}>
      <ThemeProvider>
        <App />
      </ThemeProvider>
    </QueryClientProvider>,
  );
}
beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  vi.stubGlobal(
    "matchMedia",
    vi.fn((query: string) => ({
      matches: query === "(prefers-reduced-motion: reduce)",
      media: query,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    })),
  );
  server.use(
    http.post("http://tauri.local/set_window_theme", () =>
      HttpResponse.json(null),
    ),
    http.post("http://tauri.local/get_xinghuo_activation", () =>
      HttpResponse.json(active),
    ),
    http.post("http://tauri.local/refresh_xinghuo_status", () =>
      HttpResponse.json(active),
    ),
  );
});
afterEach(() => vi.unstubAllGlobals());

describe("星火单一服务入口", () => {
  it("顶部可以切换白天黑夜，重新挂载后记住选择", async () => {
    const first = renderApp();
    fireEvent.click(screen.getByRole("button", { name: "切换到黑夜模式" }));
    await waitFor(() =>
      expect(localStorage.getItem("xinghuo-token-theme")).toBe("dark"),
    );
    expect(document.documentElement).toHaveClass("dark");
    first.unmount();
    renderApp();
    fireEvent.click(screen.getByRole("button", { name: "切换到白天模式" }));
    await waitFor(() =>
      expect(localStorage.getItem("xinghuo-token-theme")).toBe("light"),
    );
    expect(document.documentElement).toHaveClass("light");
  });
  it("旧供应商页面状态不会重新打开列表或配置编辑器", async () => {
    localStorage.setItem("xinghuo-token-last-view", "universal");
    renderApp();
    expect(await screen.findByText("sk-test…0000")).toBeInTheDocument();
    expect(screen.getByText("Codex")).toBeInTheDocument();
    expect(
      screen.queryByText("provider.addNewProvider"),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("Gemini")).not.toBeInTheDocument();
    expect(screen.queryByText("auth.json")).not.toBeInTheDocument();
    expect(screen.queryByText("config.toml")).not.toBeInTheDocument();
  });

  it("一键按钮直接调用配置命令，只发送所选工具名称", async () => {
    const useApp = vi.fn();
    server.use(
      http.post("http://tauri.local/use_xinghuo_app", async ({ request }) => {
        useApp(await request.json());
        return HttpResponse.json({
          view: active,
          apps: [{ app: "codex", ok: true }],
          renewed: false,
        });
      }),
    );
    renderApp();
    await screen.findByText("sk-test…0000");
    fireEvent.click(await screen.findByText("Codex"));
    fireEvent.click(screen.getByText("一键接入"));
    await waitFor(() => expect(useApp).toHaveBeenCalledWith({ app: "codex" }));
    await waitFor(() =>
      expect(mocks.success).toHaveBeenCalledWith(
        "已连接上 【ChatGPT】 官方服务器，请放心使用",
      ),
    );
  });

  it("无密钥直接显示完整主界面，点击一键接入才展开输入", async () => {
    server.use(
      http.post("http://tauri.local/get_xinghuo_activation", () =>
        HttpResponse.json({ activated: false, apps: [], hasWallet: false }),
      ),
    );
    renderApp();
    expect(await screen.findByText("Codex")).toBeInTheDocument();
    expect(screen.getByText("Claude Desktop")).toBeInTheDocument();
    expect(screen.getByText("Claude Desktop 接口")).toBeEnabled();
    expect(
      screen.queryByLabelText("activation.inputPlaceholder"),
    ).not.toBeInTheDocument();
    expect(screen.queryByText(/欢迎使用/)).not.toBeInTheDocument();
    for (const name of [
      "sessionManager.title",
      "skills.title",
      "mcp.title",
      "prompts.title",
    ]) {
      expect(screen.getByRole("button", { name })).toBeEnabled();
    }
    fireEvent.click(screen.getByText("Codex"));
    expect(
      screen.queryByLabelText("activation.inputPlaceholder"),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByText("一键接入"));
    expect(
      await screen.findByLabelText("activation.inputPlaceholder"),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(
      screen.queryByLabelText("activation.inputPlaceholder"),
    ).not.toBeInTheDocument();
    expect(screen.getByText("Codex")).toBeEnabled();
  });

  it("跳过不改变主界面，未激活可浏览功能，重新挂载也不拦截", async () => {
    const activate = vi.fn();
    const useApp = vi.fn();
    server.use(
      http.post("http://tauri.local/get_xinghuo_activation", () =>
        HttpResponse.json({ activated: false, apps: [], hasWallet: false }),
      ),
      http.post("http://tauri.local/activate_xinghuo", () => {
        activate();
        return HttpResponse.json({ view: active, apps: [], renewed: false });
      }),
      http.post("http://tauri.local/use_xinghuo_app", () => {
        useApp();
        return HttpResponse.json({ view: active, apps: [], renewed: false });
      }),
    );
    localStorage.setItem("xinghuo-token-activation-skipped", "true");
    const first = renderApp();
    fireEvent.click(await screen.findByText("Codex"));
    fireEvent.click(screen.getByText("一键接入"));
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    fireEvent.click(screen.getByText("sessionManager.title"));
    expect(await screen.findByTestId("sessions-app")).toHaveTextContent(
      "claude",
    );
    fireEvent.click(screen.getByRole("button", { name: "common.back" }));
    fireEvent.click(screen.getByText("skills.title"));
    expect(await screen.findByTestId("skills-panel")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "common.back" }));
    fireEvent.click(screen.getByRole("button", { name: "common.settings" }));
    fireEvent.click(await screen.findByText("close-settings"));
    expect(screen.getByText("Codex")).toBeEnabled();
    first.unmount();
    renderApp();
    expect(await screen.findByText("Codex")).toBeEnabled();
    expect(
      screen.queryByLabelText("activation.inputPlaceholder"),
    ).not.toBeInTheDocument();
    expect(screen.queryByText(/欢迎使用/)).not.toBeInTheDocument();
    expect(activate).not.toHaveBeenCalled();
    expect(useApp).not.toHaveBeenCalled();
  });

  it("保留设置入口和返回首页", async () => {
    renderApp();
    fireEvent.click(screen.getByRole("button", { name: "common.settings" }));
    fireEvent.click(await screen.findByText("close-settings"));
    expect(await screen.findByText("Codex")).toBeInTheDocument();
  });

  it("保留后台同步失败通知", async () => {
    renderApp();
    await screen.findByText("Codex");
    emitTauriEvent("webdav-sync-status-updated", null);
    expect(mocks.error).not.toHaveBeenCalled();
    emitTauriEvent("webdav-sync-status-updated", {
      source: "auto",
      status: "error",
      error: "timeout",
    });
    await waitFor(() =>
      expect(mocks.error).toHaveBeenCalledWith("自动同步失败：timeout"),
    );
  });

  it("保留技能检查更新并通过面板保护进入发现页", async () => {
    renderApp();
    fireEvent.click(screen.getByText("skills.title"));
    expect(await screen.findByTestId("skills-panel")).toBeInTheDocument();
    const updates = screen.getByText("skills.checkUpdates");
    await waitFor(() => expect(updates).toBeEnabled());
    fireEvent.click(updates);
    expect(mocks.checkUpdates).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByText("skills.discover"));
    expect(mocks.openDiscovery).toHaveBeenCalledTimes(1);
  });

  it("会话只能选择已支持的工具，旧的隐藏工具状态回退到 Claude", async () => {
    localStorage.setItem("xinghuo-token-last-app", "gemini");
    renderApp();
    fireEvent.click(screen.getByText("sessionManager.title"));
    expect(await screen.findByTestId("sessions-app")).toHaveTextContent(
      "claude",
    );
    fireEvent.click(screen.getByText("Codex"));
    expect(screen.getByTestId("sessions-app")).toHaveTextContent("codex");
    expect(screen.queryByText("Gemini")).not.toBeInTheDocument();
  });
});
