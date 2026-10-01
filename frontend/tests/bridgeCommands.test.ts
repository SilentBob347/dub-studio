import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { listSources } from "./scanHardcoded";
import { BUILT_IN_COMMANDS } from "../src/lib/mcpBridge";
import { EDITOR_FIELDS } from "../src/components/editorBridge";

const SERVER = join(import.meta.dirname, "..", "..", "crates", "dub-server", "src");
const WINDOW_TOOLS = readFileSync(join(SERVER, "mcp", "window.rs"), "utf8");
const TOOLS = readFileSync(join(SERVER, "mcp.rs"), "utf8");

/** Each tool of the window: its name and the command it asks the page for. */
const windowTools = [...WINDOW_TOOLS.matchAll(/name: "((?:ui|editor)_\w+)",[\s\S]*?call: \|args\| window\("(\w+)"/g)].map((m) => ({ name: m[1], command: m[2] }));

/** The commands the page registers with useBridgeCommand. */
const pageCommands = new Set(listSources().flatMap((file) => [...readFileSync(file, "utf8").matchAll(/useBridgeCommand\("(\w+)"/g)].map((m) => m[1])));

/** The top-level property names of a tool's JSON schema, as written in its Rust source. */
function schemaFields(source: string, tool: string): string[] {
  const start = source.indexOf(`name: "${tool}",`);
  if (start < 0) throw new Error(`no tool ${tool}`);
  const block = source.slice(start, source.indexOf("call:", start));
  const twin = /like\("(\w+)"/.exec(block);
  if (twin) return schemaFields(TOOLS, twin[1]).filter((field) => field !== "pid" && field !== "response_format");
  if (/schema: nothing/.test(block)) return [];
  if (/schema: project_only/.test(block)) return ["pid"];
  const open = block.indexOf("json!({");
  if (open < 0) throw new Error(`${tool}: its schema is not written out`);
  const fields: string[] = [];
  let depth = 0;
  for (let at = open + "json!(".length; at < block.length; at++) {
    const c = block[at];
    if (c === "{" || c === "[" || c === "(") depth++;
    else if (c === "}" || c === "]" || c === ")") { depth--; if (depth === 0) break; }
    else if (c === '"' && depth === 1) {
      const end = block.indexOf('"', at + 1);
      const key = block.slice(at + 1, end);
      if (/^\s*:/.test(block.slice(end + 1))) fields.push(key);
      at = end;
    } else if (c === '"') at = block.indexOf('"', at + 1);
  }
  return fields;
}

describe("the window's MCP tools and the page", () => {
  it("reads the tools from the server's source", () => {
    expect(windowTools.length).toBeGreaterThan(30);
    expect(windowTools.some((tool) => tool.name === "editor_segment_split")).toBe(true);
  });

  it("every command a tool asks for is answered by the page", () => {
    const unanswered = windowTools.filter((tool) => !pageCommands.has(tool.command) && !BUILT_IN_COMMANDS.includes(tool.command));
    expect(unanswered).toEqual([]);
  });

  it("every command the page answers is a tool's", () => {
    const asked = new Set(windowTools.map((tool) => tool.command));
    expect([...pageCommands].filter((command) => !asked.has(command))).toEqual([]);
    expect(BUILT_IN_COMMANDS.filter((command) => !asked.has(command))).toEqual([]);
  });

  it("each editor command takes exactly the fields its tool offers", () => {
    const editorTools = windowTools.filter((tool) => tool.name.startsWith("editor_") && tool.name !== "editor_open");
    expect(editorTools.map((tool) => tool.name).sort()).toEqual(Object.keys(EDITOR_FIELDS).sort());
    for (const tool of editorTools) {
      expect([...schemaFields(WINDOW_TOOLS, tool.name)].sort(), tool.name).toEqual([...EDITOR_FIELDS[tool.name as keyof typeof EDITOR_FIELDS]].sort());
    }
  });
});
