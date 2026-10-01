import i18next from "i18next";
import { beforeAll, describe, expect, it } from "vitest";
import en from "../src/locales/en.json";
import ru from "../src/locales/ru.json";
import zh from "../src/locales/zh.json";
import es from "../src/locales/es.json";
import pt from "../src/locales/pt.json";
import fr from "../src/locales/fr.json";

beforeAll(async () => {
  await i18next.init({
    lng: "en",
    fallbackLng: "en",
    defaultNS: "t",
    interpolation: { escapeValue: false },
    resources: { en: { t: en }, ru: { t: ru }, zh: { t: zh }, es: { t: es }, pt: { t: pt }, fr: { t: fr } },
  });
});

const tr = (lng: string, key: string, count: number) => i18next.t(key, { lng, count });

describe("plural rendering", () => {
  it("picks the right form per language", () => {
    expect(tr("en", "casting.lines", 1)).toBe("1 line");
    expect(tr("en", "casting.lines", 5)).toBe("5 lines");
    expect(tr("ru", "casting.lines", 1)).toBe("1 реплика");
    expect(tr("ru", "casting.lines", 3)).toBe("3 реплики");
    expect(tr("ru", "casting.lines", 5)).toBe("5 реплик");
    expect(tr("ru", "casting.lines", 21)).toBe("21 реплика");
    expect(tr("zh", "casting.lines", 1)).toBe("1 句台词");
    expect(tr("es", "casting.count", 1)).toBe("1 personaje");
    expect(tr("es", "casting.count", 2)).toBe("2 personajes");
    expect(tr("fr", "casting.count", 0)).toBe("0 personnage");
    expect(tr("pt", "casting.count", 3)).toBe("3 personagens");
  });
});
