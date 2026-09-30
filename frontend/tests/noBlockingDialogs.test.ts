import { readFileSync } from "node:fs";
import { relative } from "node:path";
import { describe, expect, it } from "vitest";
import { SRC_DIR, listSources } from "./scanHardcoded";

// window.confirm/alert/prompt stop the page's script: the MCP bridge's answers wait behind them, and an agent sees
// neither the question in ui_read_page nor on a screenshot. Questions go through store.askConfirm (ConfirmDialog).
const BLOCKING = /(?:^|[^\w$.])(?:window\s*\.\s*)?(confirm|alert|prompt)\s*\(|window\s*\.\s*(confirm|alert|prompt)\b/;

const withoutComments = (code: string) => code.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:"'`])\/\/.*$/gm, "$1");

describe("no blocking browser dialogs", () => {
  it("the page never calls window.confirm, alert or prompt", () => {
    const found: string[] = [];
    for (const file of listSources()) {
      withoutComments(readFileSync(file, "utf8")).split("\n").forEach((line, at) => {
        if (BLOCKING.test(line)) found.push(`${relative(SRC_DIR, file)}:${at + 1}: ${line.trim()}`);
      });
    }
    expect(found).toEqual([]);
  });

  it("the check catches each form", () => {
    for (const code of ["if (!window.confirm(x)) return;", "alert(\"x\")", "const v = prompt(q);", "window.alert"]) expect(BLOCKING.test(code), code).toBe(true);
    for (const code of ["await askConfirm({})", "answerConfirm(true)", "obj.confirm()", "confirmLabel: x"]) expect(BLOCKING.test(code), code).toBe(false);
  });
});
