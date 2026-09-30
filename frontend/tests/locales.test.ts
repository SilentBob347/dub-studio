import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { SAME_AS_EN } from "./i18nAllowlist";

const LANGS = ["en", "ru", "zh", "es", "pt", "fr"] as const;
type Lang = (typeof LANGS)[number];
const LOCALES_DIR = join(import.meta.dirname, "..", "src", "locales");

type Json = string | Json[] | { [k: string]: Json };

const load = (lang: Lang): Record<string, string> => {
  const flat: Record<string, string> = {};
  const walk = (node: Json, path: string) => {
    if (typeof node === "string") flat[path] = node;
    else if (Array.isArray(node)) node.forEach((v, i) => walk(v, `${path}[${i}]`));
    else for (const [k, v] of Object.entries(node)) walk(v, path ? `${path}.${k}` : k);
  };
  walk(JSON.parse(readFileSync(join(LOCALES_DIR, `${lang}.json`), "utf8")) as Json, "");
  return flat;
};

const BUNDLES = Object.fromEntries(LANGS.map((l) => [l, load(l)])) as Record<Lang, Record<string, string>>;

const PLURAL = /^(.*)_(zero|one|two|few|many|other)$/;
const isPlural = (key: string) => PLURAL.test(key);
const pluralBase = (key: string) => key.replace(PLURAL, "$1");

const plainKeys = (b: Record<string, string>) => Object.keys(b).filter((k) => !isPlural(k)).sort();
const pluralBases = (b: Record<string, string>) => [...new Set(Object.keys(b).filter(isPlural).map(pluralBase))].sort();
const categoriesOf = (b: Record<string, string>, base: string) =>
  Object.keys(b).filter((k) => isPlural(k) && pluralBase(k) === base).map((k) => k.slice(base.length + 1)).sort();

const placeholders = (s: string) => [...new Set([...s.matchAll(/\{\{\s*([\w.]+)\s*\}\}/g)].map((m) => m[1]))].sort();
const tags = (s: string) => [...s.matchAll(/<\/?[a-z]+>/g)].map((m) => m[0]).sort();
const CYRILLIC = /[Ѐ-ӿ]/;

const en = BUNDLES.en;
const others = LANGS.filter((l) => l !== "en");

describe("locales", () => {
  for (const lang of LANGS) {
    const b = BUNDLES[lang];

    describe(lang, () => {
      it("has exactly the keys of en, plural groups aside", () => {
        expect(plainKeys(b)).toEqual(plainKeys(en));
        expect(pluralBases(b)).toEqual(pluralBases(en));
      });

      it("has exactly the plural categories of its language", () => {
        const expected = new Intl.PluralRules(lang).resolvedOptions().pluralCategories.slice().sort();
        for (const base of pluralBases(en)) expect(categoriesOf(b, base), base).toEqual(expected);
      });

      it("has no empty values", () => {
        expect(Object.entries(b).filter(([, v]) => v.trim() === "").map(([k]) => k)).toEqual([]);
      });

      it("keeps interpolation placeholders and markup tags of en", () => {
        const bad: string[] = [];
        for (const [key, v] of Object.entries(b)) {
          const ref = isPlural(key) ? en[`${pluralBase(key)}_other`] : en[key];
          if (JSON.stringify(placeholders(v)) !== JSON.stringify(placeholders(ref))) bad.push(`${key}: placeholders`);
          if (JSON.stringify(tags(v)) !== JSON.stringify(tags(ref))) bad.push(`${key}: tags`);
        }
        expect(bad).toEqual([]);
      });
    });
  }

  for (const lang of LANGS.filter((l) => l !== "ru")) {
    it(`${lang} contains no Cyrillic`, () => {
      expect(Object.entries(BUNDLES[lang]).filter(([, v]) => CYRILLIC.test(v)).map(([k]) => k)).toEqual([]);
    });
  }

  it("ru is written in Cyrillic", () => {
    const missing = Object.entries(BUNDLES.ru).filter(([k, v]) => !CYRILLIC.test(v) && !(SAME_AS_EN["*"].includes(k) || SAME_AS_EN.ru.includes(k)));
    expect(missing.map(([k]) => k)).toEqual([]);
  });

  it("the allowlist of identical-to-en values for all languages has no stale entries", () => {
    const stale = SAME_AS_EN["*"].filter((k) => !others.some((lang) => BUNDLES[lang][k] === (isPlural(k) ? en[k] ?? en[`${pluralBase(k)}_other`] : en[k])));
    expect(stale).toEqual([]);
  });

  for (const lang of others) {
    const allowed = new Set([...SAME_AS_EN["*"], ...SAME_AS_EN[lang]]);
    const refFor = (key: string) => (isPlural(key) ? en[key] ?? en[`${pluralBase(key)}_other`] : en[key]);

    it(`${lang} does not leave English text untranslated`, () => {
      const same = Object.entries(BUNDLES[lang]).filter(([k, v]) => v === refFor(k)).map(([k]) => k);
      expect(same.filter((k) => !allowed.has(k))).toEqual([]);
    });

    it(`${lang} allowlist of identical-to-en values has no stale entries`, () => {
      const same = new Set(Object.entries(BUNDLES[lang]).filter(([k, v]) => v === refFor(k)).map(([k]) => k));
      expect([...SAME_AS_EN[lang]].filter((k) => !same.has(k))).toEqual([]);
    });
  }
});
