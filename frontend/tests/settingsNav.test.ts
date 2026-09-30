import { describe, expect, it } from "vitest";
import { parseSettingsTarget, SETTINGS_SECTIONS } from "../src/lib/settingsNav";

describe("opening the settings at a section", () => {
  const all = SETTINGS_SECTIONS;

  it("opens a section and the part of it to scroll to", () => {
    expect(parseSettingsTarget("cloud:key", all)).toEqual({ section: "cloud", part: "key" });
    expect(parseSettingsTarget("about", all)).toEqual({ section: "about", part: null });
  });

  it("does not open a section the window does not have", () => {
    expect(parseSettingsTarget("karaoke", all)).toBeNull();
    expect(parseSettingsTarget("agent", all.filter((s) => s !== "agent"))).toBeNull();
    expect(parseSettingsTarget("", all)).toBeNull();
  });
});
