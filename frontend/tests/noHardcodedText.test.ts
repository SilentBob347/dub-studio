import { describe, expect, it } from "vitest";
import { ALLOW } from "./i18nAllowlist";
import { scanAll, type Violation } from "./scanHardcoded";

const fmt = (v: Violation[]) => v.map((x) => `${x.file}:${x.line} [${x.kind}] ${x.text}`).join("\n");

describe("no hardcoded UI text", () => {
  const used = new Set<string>();
  const violations = scanAll(ALLOW, used);

  it("has no human-readable literals outside t()", () => {
    expect(fmt(violations)).toBe("");
  });

  it("keeps the token allowlist free of stale entries", () => {
    const stale = [...ALLOW.tokens].filter((k) => !used.has(k));
    expect(stale).toEqual([]);
  });
});
