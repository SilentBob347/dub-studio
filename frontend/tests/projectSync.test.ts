import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { TFunction } from "i18next";
import { api, WINDOW_ID, type Project } from "../src/lib/api";
import { useStore } from "../src/store";
import { projectSync } from "../src/components/editorBridge";

const t = ((key: string) => key) as unknown as TFunction<"t">;

function project(text: string): Project {
  return {
    segments: [{ id: "s1", start: 0, end: 1, speaker: "1", src_text: "hi", tgt_text: text, dirty: false }],
    captions: { blur_boxes: [], titles: [], overrides: [] },
  } as unknown as Project;
}

/** What the studio answers: the project stored now at its revision, and every request the window made. */
const studio = { stored: project("agent's"), rev: 1 };
const requests: Array<{ method: string; url: string; rev: string | null }> = [];

beforeAll(() => {
  vi.stubGlobal("fetch", vi.fn(async (url: string, init: RequestInit = {}) => {
    const method = init.method ?? "GET";
    requests.push({ method, url, rev: new Headers(init.headers).get("x-project-rev") });
    if (method !== "GET") studio.rev += 1;
    return new Response(JSON.stringify(studio.stored), { status: 200, headers: { "content-type": "application/json", "x-project-rev": String(studio.rev) } });
  }));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

beforeEach(() => {
  requests.length = 0;
  useStore.setState({ pid: "p1", project: null, past: [], future: [], activities: [] });
});

describe("the window after a save behind it", () => {
  it("ends the undo history on someone else's save even when it already shows that save", async () => {
    const shown = project("agent's");
    useStore.setState({ project: shown, past: [{ project: project("before"), rev: 0 }] });
    await projectSync("p1", () => null, t)({ changed: "project", pid: "p1", rev: 1, by: "agent" });
    const s = useStore.getState();
    expect(s.past).toEqual([]);
    expect(s.future).toEqual([]);
    expect(s.project).toBe(shown);
    expect(s.activities.at(-1)).toMatchObject({ text: "bridge.changedByAgent", kind: "agent" });
  });

  it("ends it after a notice the window may have missed too, and reads the project that changed", async () => {
    useStore.setState({ project: project("mine"), future: [{ project: project("later"), rev: 0 }] });
    await projectSync("p1", () => null, t)({ changed: "everything" });
    const s = useStore.getState();
    expect(s.future).toEqual([]);
    expect(s.project?.segments[0].tgt_text).toBe("agent's");
    expect(s.activities.at(-1)?.text).toBe("bridge.changedWhileAway");
  });

  it("keeps the history on its own save it already shows, without reading again", async () => {
    await api.getProject("p1");
    const past = [{ project: project("before"), rev: 0 }];
    useStore.setState({ project: project("agent's"), past });
    requests.length = 0;
    await projectSync("p1", () => null, t)({ changed: "project", pid: "p1", rev: studio.rev, by: `window:${WINDOW_ID}` });
    expect(requests).toEqual([]);
    expect(useStore.getState().past).toBe(past);
  });

  it("takes the result of a job it started itself without leaving the finished video it shows", async () => {
    await api.getProject("p1");
    const past = [{ project: project("before"), rev: 0 }];
    useStore.setState({ project: project("mine"), past, rendered: true, rev: 7 });
    studio.rev += 1;
    studio.stored = project("voiced");
    await projectSync("p1", () => null, t)({ changed: "project", pid: "p1", rev: studio.rev, by: `window:${WINDOW_ID}`, job: true });
    const s = useStore.getState();
    expect(s.project?.segments[0].tgt_text).toBe("voiced");
    expect(s.rev).toBe(7);
    expect(s.rendered).toBe(true);
    expect(s.past).toBe(past);
    expect(s.activities).toEqual([]);
    studio.stored = project("agent's");
  });
});

describe("undo's snapshots", () => {
  it("carry the revision they were taken at, and the PUT of one names it instead of the latest", async () => {
    await api.getProject("p1");
    const taken = studio.rev;
    const before = project("before");
    useStore.setState({ project: before });
    useStore.getState().pushHistory(before);
    expect(useStore.getState().past[0].rev).toBe(taken);

    // the window's edit lands on top of a save it has not heard of: its answer brings a newer revision
    studio.rev += 1;
    useStore.getState().setProject(await api.patch("p1", { op: "segment", id: "s1", tgt_text: "mine" }));
    const snapshot = useStore.getState().undo()!;
    requests.length = 0;
    await api.putProject("p1", snapshot.project, snapshot.rev);
    await api.putProject("p1", before);
    expect(requests.map((r) => r.rev)).toEqual([String(taken), String(taken + 3)]);
  });
});
