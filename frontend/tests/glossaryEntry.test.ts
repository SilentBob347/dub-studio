import { describe, expect, it } from "vitest";
import type { GlossaryEntry } from "../src/lib/api";
import { editedEntry, servesLang } from "../src/lib/glossaryEntry";

const ru: GlossaryEntry = {
  term: "Harry", translation: "Гарри", keep: false, pronunciation: "", asr_fix: [], note: "", source: "auto", lang: "ru",
};

describe("editedEntry", () => {
  it("writes the translation and the pronunciation in the project's language", () => {
    expect(editedEntry(ru, { translation: "Harry" }, "es")).toMatchObject({ translation: "Harry", lang: "es", source: "manual" });
    expect(editedEntry(ru, { pronunciation: "Jari" }, "es")).toMatchObject({ pronunciation: "Jari", lang: "es" });
  });
  it("keeps the language for edits that do not depend on it", () => {
    expect(editedEntry(ru, { note: "hero" }, "es").lang).toBe("ru");
    expect(editedEntry(ru, { term: "Harry P." }, "es").lang).toBe("ru");
    expect(editedEntry(ru, { keep: true }, "es").lang).toBe("ru");
  });
});

describe("servesLang", () => {
  it("matches the primary language and takes an entry without one for any", () => {
    expect(servesLang("ru", "ru")).toBe(true);
    expect(servesLang("pt-BR", "pt_PT")).toBe(true);
    expect(servesLang("", "es")).toBe(true);
    expect(servesLang("ru", "es")).toBe(false);
  });
});
