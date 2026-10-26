import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const {
  checkAppUpdate,
  getToolVersions,
  openExternal,
  getVersion,
  toastSuccess,
  toastError,
} = vi.hoisted(() => ({
  checkAppUpdate: vi.fn(),
  getToolVersions: vi.fn(),
  openExternal: vi.fn(),
  getVersion: vi.fn(),
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));

vi.mock("@/lib/api/update", () => ({ checkAppUpdate }));
vi.mock("@/lib/api", () => ({
  settingsApi: { getToolVersions, openExternal },
}));
vi.mock("@tauri-apps/api/app", () => ({ getVersion }));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, unknown>) =>
      options ? `${key}:${JSON.stringify(options)}` : key,
  }),
}));
vi.mock("sonner", () => ({
  toast: { success: toastSuccess, error: toastError, info: vi.fn() },
}));

import { AboutSection } from "@/components/settings/AboutSection";

const result = {
  available: true,
  currentVersion: "3.20.4",
  latestVersion: "3.21.0",
  downloadUrl: "https://gitee.com/wfpromax/xinghuo-token/releases/v3.21.0",
  source: "Gitee",
};

describe("AboutSection app update check", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getVersion.mockResolvedValue("3.20.4");
    getToolVersions.mockResolvedValue([]);
  });

  it("offers the Gitee download when a newer version is found", async () => {
    checkAppUpdate.mockResolvedValue(result);
    render(<AboutSection isPortable={false} />);
    fireEvent.click(
      await screen.findByRole("button", { name: "settings.checkForUpdates" }),
    );

    await waitFor(() => expect(toastSuccess).toHaveBeenCalled());
    const [message, options] = toastSuccess.mock.calls[0];
    expect(message).toContain("settings.updateAvailable");
    expect(message).toContain("3.21.0");
    options.action.onClick();
    expect(openExternal).toHaveBeenCalledWith(result.downloadUrl);
  });

  it("reports an up-to-date app", async () => {
    checkAppUpdate.mockResolvedValue({ ...result, available: false });
    render(<AboutSection isPortable={false} />);
    fireEvent.click(
      await screen.findByRole("button", { name: "settings.checkForUpdates" }),
    );
    await waitFor(() =>
      expect(toastSuccess).toHaveBeenCalledWith("settings.upToDate", {
        closeButton: true,
      }),
    );
  });

  it("reports manual check failures", async () => {
    checkAppUpdate.mockRejectedValue(new Error("Gitee unavailable"));
    render(<AboutSection isPortable={false} />);
    fireEvent.click(
      await screen.findByRole("button", { name: "settings.checkForUpdates" }),
    );
    await waitFor(() =>
      expect(toastError).toHaveBeenCalledWith("settings.checkUpdateFailed", {
        description: "Gitee unavailable",
        closeButton: true,
      }),
    );
  });
});
