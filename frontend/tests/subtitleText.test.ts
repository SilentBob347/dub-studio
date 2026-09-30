import { describe, expect, it } from "vitest";
import { subtitleText } from "../src/lib/subtitleText";

const seg = { src_text: " Where were you? ", tgt_text: "Где ты был?" };

describe("subtitleText", () => {
  it("shows the translation, or the original under a dub when the subtitles are in the original language", () => {
    expect(subtitleText(seg, "translate", true, false)).toBe("Где ты был?");
    expect(subtitleText(seg, "transcribe", true, false)).toBe("Where were you?");
    expect(subtitleText({ src_text: "Hi", tgt_text: "Hi, you" }, "transcribe", false, false)).toBe("Hi, you");
  });

  it("puts both languages into one bilingual subtitle in the chosen order", () => {
    expect(subtitleText(seg, "bilingual", true, false)).toBe("Где ты был?\nWhere were you?");
    expect(subtitleText(seg, "bilingual", true, true)).toBe("Where were you?\nГде ты был?");
    expect(subtitleText({ src_text: "OK", tgt_text: "OK" }, "bilingual", false, false)).toBe("OK");
    expect(subtitleText({ src_text: "Hello", tgt_text: "" }, "bilingual", false, false)).toBe("Hello");
  });
});
