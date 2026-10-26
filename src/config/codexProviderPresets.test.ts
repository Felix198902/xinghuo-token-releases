import { describe, expect, it } from "vitest";
import {
  codexProviderPresets,
  generateThirdPartyConfig,
} from "./codexProviderPresets";

describe("codexProviderPresets", () => {
  it("只预置星火接口，不提供官方配置", () => {
    expect(codexProviderPresets).toHaveLength(1);
    expect(codexProviderPresets[0].category).toBe("third_party");
    expect(codexProviderPresets[0].config).toContain(
      "https://ai.wfpromax.xyz/v1",
    );
  });
  it("key-based third-party template keeps the fallback flag by default", () => {
    expect(
      generateThirdPartyConfig("acme", "https://api.acme.dev/v1", "m1"),
    ).toContain("requires_openai_auth = true");
  });
});
