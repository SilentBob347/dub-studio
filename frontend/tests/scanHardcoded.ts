import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import ts from "typescript";

export const SRC_DIR = join(import.meta.dirname, "..", "src");

export type Violation = { file: string; line: number; kind: string; text: string };

export type Allow = {
  /** Technical tokens, units and brand names that are the same in every language (exact, trimmed). */
  tokens: ReadonlySet<string>;
  /** Files where Cyrillic literals are legitimate data (endonyms of content languages). */
  cyrillicFiles: ReadonlySet<string>;
  /** Files that are not UI and are not scanned. */
  skipFiles: ReadonlySet<string>;
};

const HUMAN_ATTRS = new Set(["title", "placeholder", "aria-label", "alt", "aria-description", "aria-placeholder", "label", "description", "hint", "tip", "tooltip", "caption", "heading"]);
const CYR = /[Ѐ-ӿ]/;
const LETTER = /\p{L}/u;
const PROSE = /^[A-Z][a-z]+(?:[ ,'’-][A-Za-z][A-Za-z'’-]*)+[.!?…:]?$/;
const PROP_NAMES = new Set(["label", "title", "text", "message", "msg", "description", "hint", "placeholder", "desc", "tip", "caption", "heading", "error"]);
const MESSAGE_CALLS = /^(?:[\w$.]*\.)?(pushActivity|alert|confirm|prompt|setMsg|setErr|setError|setCap|setNotice|setStatus|surfaceErr)$/;
const HEX_COLOR = /^#[0-9a-fA-F]{3,8}$/;
const CONSOLE_OBJ = new Set(["console"]);

export function listSources(dir = SRC_DIR): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) out.push(...listSources(p));
    else if (/\.(ts|tsx)$/.test(name) && !name.endsWith(".d.ts")) out.push(p);
  }
  return out.sort();
}

function isT(n: ts.Node): boolean {
  return ts.isCallExpression(n) && ts.isIdentifier(n.expression) && (n.expression.text === "t" || n.expression.text === "tt");
}

function isConsoleCall(n: ts.Node): boolean {
  return ts.isCallExpression(n) && ts.isPropertyAccessExpression(n.expression)
    && ts.isIdentifier(n.expression.expression) && CONSOLE_OBJ.has(n.expression.expression.text);
}

function isPropValue(n: ts.Node): boolean {
  const p = n.parent;
  return ts.isPropertyAssignment(p) && p.initializer === n && (ts.isIdentifier(p.name) || ts.isStringLiteral(p.name)) && PROP_NAMES.has(p.name.text);
}

function isMessageArg(n: ts.Node): boolean {
  let cur: ts.Node = n;
  while (cur.parent && (ts.isTemplateExpression(cur.parent) || ts.isTemplateSpan(cur.parent) || ts.isParenthesizedExpression(cur.parent)
    || (ts.isBinaryExpression(cur.parent) && cur.parent.operatorToken.kind === ts.SyntaxKind.PlusToken)
    || (ts.isConditionalExpression(cur.parent) && cur.parent.condition !== cur))) cur = cur.parent;
  const call = cur.parent;
  return !!call && ts.isCallExpression(call) && call.arguments[0] === cur && MESSAGE_CALLS.test(call.expression.getText());
}

function inTCall(n: ts.Node): boolean {
  for (let p = n.parent; p; p = p.parent) if (isT(p)) return true;
  return false;
}

function inConsoleOrImport(n: ts.Node): boolean {
  for (let p = n.parent; p; p = p.parent) {
    if (isConsoleCall(p) || ts.isImportDeclaration(p) || ts.isExportDeclaration(p) || ts.isImportTypeNode(p)) return true;
    if (ts.isLiteralTypeNode(p) || ts.isTypeNode(p)) return true;
  }
  return false;
}

function renderSink(n: ts.Node): ts.Node | null {
  let cur: ts.Node = n;
  for (let p = cur.parent; p; cur = p, p = p.parent) {
    if (ts.isParenthesizedExpression(p) || ts.isAsExpression(p) || ts.isNonNullExpression(p) || ts.isTemplateExpression(p) || ts.isTemplateSpan(p)) continue;
    if (ts.isConditionalExpression(p)) { if (p.condition === cur) return null; continue; }
    if (ts.isBinaryExpression(p)) {
      const op = p.operatorToken.kind;
      const passes = op === ts.SyntaxKind.AmpersandAmpersandToken || op === ts.SyntaxKind.BarBarToken || op === ts.SyntaxKind.QuestionQuestionToken || op === ts.SyntaxKind.PlusToken;
      if (!passes || (op === ts.SyntaxKind.AmpersandAmpersandToken && p.left === cur)) return null;
      continue;
    }
    return p;
  }
  return null;
}

function attrOf(n: ts.Node): string | null {
  const sink = renderSink(n);
  if (!sink) return null;
  if (ts.isJsxAttribute(sink)) return ts.isIdentifier(sink.name) ? sink.name.text : sink.name.getText();
  if (ts.isJsxExpression(sink) && ts.isJsxAttribute(sink.parent)) return ts.isIdentifier(sink.parent.name) ? sink.parent.name.text : sink.parent.name.getText();
  return null;
}

function flowsToJsxChild(n: ts.Node): boolean {
  const sink = renderSink(n);
  return !!sink && ts.isJsxExpression(sink) && (ts.isJsxElement(sink.parent) || ts.isJsxFragment(sink.parent));
}

export function scanFile(file: string, allow: Allow, used?: Set<string>): Violation[] {
  const rel = relative(SRC_DIR, file).replaceAll("\\", "/");
  if (allow.skipFiles.has(rel)) return [];
  const text = readFileSync(file, "utf8");
  const sf = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, file.endsWith("x") ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  const out: Violation[] = [];
  const cyrAllowed = allow.cyrillicFiles.has(rel);
  const add = (n: ts.Node, kind: string, value: string) => {
    const { line } = sf.getLineAndCharacterOfPosition(n.getStart(sf));
    out.push({ file: rel, line: line + 1, kind, text: value.replace(/\s+/g, " ").trim().slice(0, 90) });
  };
  const tokenOk = (v: string) => {
    const k = v.replace(/\s+/g, " ").trim();
    if (!allow.tokens.has(k)) return false;
    used?.add(k);
    return true;
  };

  const visit = (n: ts.Node) => {
    if (ts.isJsxText(n)) {
      const v = n.getText(sf);
      if (LETTER.test(v) && !tokenOk(v)) add(n, "jsx-text", v);
    } else if (ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n) || ts.isTemplateHead(n) || ts.isTemplateMiddle(n) || ts.isTemplateTail(n)) {
      const v = n.text;
      if (HEX_COLOR.test(v.trim()) || (LETTER.test(v) && tokenOk(v))) { /* not text */ }
      else if (!inConsoleOrImport(n) && !(ts.isStringLiteral(n) && ts.isJsxAttribute(n.parent) && !HUMAN_ATTRS.has(n.parent.name.getText()))) {
        const attr = attrOf(n);
        if (CYR.test(v) && !cyrAllowed) add(n, "cyrillic", v);
        else if (!inTCall(n) && LETTER.test(v)) {
          if (attr && HUMAN_ATTRS.has(attr) && !tokenOk(v)) add(n, `attr-${attr}`, v);
          else if (!attr && flowsToJsxChild(n) && !tokenOk(v)) add(n, "jsx-expr", v);
          else if (!attr && isPropValue(n) && !tokenOk(v)) add(n, "prop", v);
          else if (!attr && isMessageArg(n) && !tokenOk(v)) add(n, "message-call", v);
          else if (!attr && PROSE.test(v.trim()) && !tokenOk(v)) add(n, "prose", v);
        }
      }
    }
    ts.forEachChild(n, visit);
  };
  visit(sf);
  return out;
}

export function scanAll(allow: Allow, used?: Set<string>): Violation[] {
  return listSources().flatMap((f) => scanFile(f, allow, used));
}
