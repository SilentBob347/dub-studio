import type { GlossaryEntry } from "./api";

const primary = (code: string) => code.trim().split(/[-_]/)[0].toLowerCase();

/** The entry's translation and pronunciation serve the target language (an entry without a language serves any). */
export const servesLang = (entryLang: string, target: string) => !primary(entryLang) || primary(entryLang) === primary(target);

// Перевод и произношение применяются только на языке записи: правка пишет их на языке проекта.
export function editedEntry(entry: GlossaryEntry, patch: Partial<GlossaryEntry>, tgtLang: string): GlossaryEntry {
  const spoken = "translation" in patch || "pronunciation" in patch;
  return { ...entry, ...patch, source: "manual", ...(spoken ? { lang: tgtLang } : {}) };
}
