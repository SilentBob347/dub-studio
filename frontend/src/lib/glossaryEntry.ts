import type { GlossaryEntry } from "./api";

// Перевод и произношение применяются только на языке записи: правка пишет их на языке проекта.
export function editedEntry(entry: GlossaryEntry, patch: Partial<GlossaryEntry>, tgtLang: string): GlossaryEntry {
  const spoken = "translation" in patch || "pronunciation" in patch;
  return { ...entry, ...patch, source: "manual", ...(spoken ? { lang: tgtLang } : {}) };
}
