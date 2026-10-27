import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { exportDiagnostics, openExternal } = vi.hoisted(() => ({
  exportDiagnostics: vi.fn(),
  openExternal: vi.fn(),
}));

vi.mock("@/lib/api/xinghuo", () => ({
  xinghuoApi: { exportDiagnostics },
}));
vi.mock("@/lib/api", () => ({ settingsApi: { openExternal } }));

import { SupportContacts } from "@/components/activation/SupportContacts";

describe("SupportContacts diagnostics", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("exports a debug zip and shows its path", async () => {
    exportDiagnostics.mockResolvedValue(
      "/Users/test/Desktop/xinghuo-debug.zip",
    );
    render(<SupportContacts contact="QQ 169106354" />);
    fireEvent.click(screen.getByRole("button", { name: "一键打包调试日志" }));

    expect(await screen.findByText(/xinghuo-debug.zip/)).toBeInTheDocument();
    expect(exportDiagnostics).toHaveBeenCalledTimes(1);
  });

  it("shows an error when export fails", async () => {
    exportDiagnostics.mockRejectedValue(new Error("disk full"));
    render(<SupportContacts contact="QQ 169106354" />);
    fireEvent.click(screen.getByRole("button", { name: "一键打包调试日志" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("disk full");
  });
});
