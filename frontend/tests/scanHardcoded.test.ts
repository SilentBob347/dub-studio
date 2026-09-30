import { describe, expect, it } from "vitest";
import { scanSource, type Allow } from "./scanHardcoded";

const ALLOW: Allow = { tokens: new Set(["GPU"]), cyrillicFiles: new Set(["data.ts"]), skipFiles: new Set(["skip.ts"]) };

const kinds = (code: string, rel = "x.tsx") => scanSource(code, rel, ALLOW).map((v) => v.kind);

describe("scanSource flags", () => {
  it("jsx text", () => expect(kinds(`const a = <div>Hello world</div>;`)).toEqual(["jsx-text"]));
  it("human attribute literal", () => {
    expect(kinds(`const a = <input placeholder="Type here" />;`)).toEqual(["attr-placeholder"]);
    expect(kinds(`const a = <b title="Save" />;`)).toEqual(["attr-title"]);
    expect(kinds(`const a = <b aria-label="Close" />;`)).toEqual(["attr-aria-label"]);
    expect(kinds(`const a = <img alt="Logo" />;`)).toEqual(["attr-alt"]);
  });
  it("human attribute expression", () => expect(kinds("const a = <b title={ok ? \"Yes\" : \"No\"} />;")).toEqual(["attr-title", "attr-title"]));
  it("cyrillic in a custom JSX attribute", () => expect(kinds(`const a = <Sel empty="— выбрать модель —" />;`)).toEqual(["cyrillic"]));
  it("cyrillic in an expression attribute", () => expect(kinds(`const a = <Sel empty={"— выбрать модель —"} />;`)).toEqual(["cyrillic"]));
  it("cyrillic in plain code", () => expect(kinds(`const s = "Готово";`, "x.ts")).toEqual(["cyrillic"]));
  it("cyrillic in a template literal", () => expect(kinds("const s = `Ошибка ${e}`;", "x.ts")).toEqual(["cyrillic"]));
  it("literal rendered as a JSX child", () => expect(kinds(`const a = <p>{ok && "Saved"}</p>;`)).toEqual(["jsx-expr"]));
  it("object prop with a human name", () => expect(kinds(`const o = { label: "Open file" };`, "x.ts")).toEqual(["prop"]));
  it("message call argument", () => expect(kinds(`pushActivity("done");`, "x.ts")).toEqual(["message-call"]));
  it("english prose", () => expect(kinds(`const m = "Connection was lost";`, "x.ts")).toEqual(["prose"]));
  it("prose starting with an abbreviation", () => expect(kinds(`throw new Error("SSE connection lost");`, "x.ts")).toEqual(["prose"]));
});

describe("scanSource accepts", () => {
  it("t() calls", () => expect(kinds(`const a = <b title={t("a.b")}>{t("c.d")}</b>;`)).toEqual([]));
  it("technical attributes", () => expect(kinds(`const a = <div className="flex gap-2" data-x="abc" />;`)).toEqual([]));
  it("hex colors", () => expect(kinds(`const c = "#ff00aa";`, "x.ts")).toEqual([]));
  it("allowlisted tokens", () => expect(kinds(`const a = <span>GPU</span>;`)).toEqual([]));
  it("console output", () => expect(kinds(`console.warn("Something went wrong");`, "x.ts")).toEqual([]));
  it("imports", () => expect(kinds(`import x from "./Привет";`, "x.ts")).toEqual([]));
  it("cyrillic in an allowlisted data file", () => expect(kinds(`export const L = [{ name: "Русский" }];`, "data.ts")).toEqual([]));
  it("skipped files", () => expect(scanSource(`const s = "Готово";`, "skip.ts", ALLOW)).toEqual([]));
  it("whitespace-only jsx text", () => expect(kinds(`const a = <div>\n  <b />\n</div>;`)).toEqual([]));
});
