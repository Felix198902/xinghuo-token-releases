import { describe, expect, it } from "vitest";

import { piProviderPresets } from "@/config/piProviderPresets";
import {
  isPiThinkingLevelMap,
  PI_THINKING_LEVELS,
  piThinkingBindings,
  piThinkingProfiles,
  resolvePiThinkingProfile,
} from "@/config/piThinkingProfiles";

describe("Pi thinking profiles", () => {
  it("keeps valid, non-empty native maps", () => {
    for (const profile of Object.values(piThinkingProfiles)) {
      expect(isPiThinkingLevelMap(profile.map)).toBe(true);
      expect(Object.keys(profile.map).length).toBeGreaterThan(0);
      expect(
        Object.keys(profile.map).every((key) =>
          PI_THINKING_LEVELS.includes(
            key as (typeof PI_THINKING_LEVELS)[number],
          ),
        ),
      ).toBe(true);
    }
  });

  it("uses only explicit catalog and API bindings", () => {
    for (const binding of piThinkingBindings) {
      expect(resolvePiThinkingProfile(binding)).toEqual({
        profileId: binding.profileId,
        map: { ...piThinkingProfiles[binding.profileId].map },
        ...(binding.modelCompat
          ? { modelCompat: { ...binding.modelCompat } }
          : {}),
      });
      expect(
        resolvePiThinkingProfile({
          ...binding,
          api:
            binding.api === "openai-responses"
              ? "openai-completions"
              : "openai-responses",
        }),
      ).toBeUndefined();
    }
  });

  it("gives every reasoning preset an explicit map", () => {
    for (const preset of piProviderPresets) {
      for (const model of preset.settingsConfig.models) {
        if (!model.reasoning) continue;
        expect(model).toHaveProperty("thinkingLevelMap");
        expect(isPiThinkingLevelMap(model.thinkingLevelMap)).toBe(true);
      }
    }
  });

  it("does not add a generic binding for host-sensitive model families", () => {
    expect(
      piThinkingBindings.some((binding) =>
        /^(deepseek|zai|moonshotai)\//.test(binding.catalogKey),
      ),
    ).toBe(false);
  });
});
