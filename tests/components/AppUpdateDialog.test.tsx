import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { checkAppUpdate, openExternal } = vi.hoisted(() => ({
  checkAppUpdate: vi.fn(),
  openExternal: vi.fn(),
}));

vi.mock("@/lib/api/update", () => ({ checkAppUpdate }));
vi.mock("@/lib/api", () => ({ settingsApi: { openExternal } }));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));

import { AppUpdateDialog } from "@/components/AppUpdateDialog";

const available = {
  available: true,
  currentVersion: "3.20.4",
  latestVersion: "3.21.0",
  title: "星火 3.21.0",
  notes: "修复激活并加入更新提示",
  downloadUrl: "https://gitee.com/wfpromax/xinghuo-token/releases/v3.21.0",
  source: "Gitee",
};

async function runStartupCheck() {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(1500);
  });
}

describe("AppUpdateDialog", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    localStorage.clear();
    checkAppUpdate.mockReset();
    openExternal.mockReset();
    openExternal.mockResolvedValue(undefined);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("shows a newer Gitee release and opens its download page", async () => {
    checkAppUpdate.mockResolvedValue(available);
    render(<AppUpdateDialog />);
    await runStartupCheck();

    expect(screen.getByText("settings.appUpdateAvailable")).toBeInTheDocument();
    expect(screen.getByText("v3.21.0")).toBeInTheDocument();
    expect(screen.getByText(available.notes)).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", { name: "settings.appUpdateDownload" }),
    );
    await act(async () => {});
    expect(openExternal).toHaveBeenCalledWith(available.downloadUrl);
  });

  it("does not repeatedly prompt for a dismissed version", async () => {
    checkAppUpdate.mockResolvedValue(available);
    const first = render(<AppUpdateDialog />);
    await runStartupCheck();
    fireEvent.click(
      screen.getByRole("button", { name: "settings.appUpdateDismiss" }),
    );
    expect(localStorage.getItem("xinghuo:update:dismissed-version")).toBe(
      "3.21.0",
    );
    first.unmount();

    render(<AppUpdateDialog />);
    await runStartupCheck();
    expect(
      screen.queryByText("settings.appUpdateAvailable"),
    ).not.toBeInTheDocument();
  });

  it("stays silent when up to date or the background check fails", async () => {
    checkAppUpdate.mockResolvedValueOnce({
      ...available,
      available: false,
      latestVersion: "3.20.4",
    });
    render(<AppUpdateDialog />);
    await runStartupCheck();
    expect(
      screen.queryByText("settings.appUpdateAvailable"),
    ).not.toBeInTheDocument();
  });
});
