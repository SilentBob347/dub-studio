import { describe, expect, it } from "vitest";
import type { ProjectListing } from "../src/lib/api";
import { facet, NO_FILTER, selectProjects, type ProjectFilter, type ProjectOrder } from "../src/lib/projectList";

const project = (pid: string, video: string, created: number | null, mtime: number, duration: number, extra: Partial<ProjectListing> = {}): ProjectListing => ({
  pid, video, created, mtime, duration, tgt_lang: "ru", mode: "dub", width: 1920, height: 1080, segments: 1, audio_only: false, done: false, job_kind: null, job_state: null, job_stage: null, job_error: null, ...extra,
});

const list = [
  project("a", "Ёлка.mp4", 20, 500, 185),
  project("b", "арфа.mp4", 30, 100, 59, { tgt_lang: "en", done: true }),
  project("c", "Бас 10.mp4", 10, 900, 720, { mode: "voiceover" }),
  project("d", "Бас 9.mp4", null, 50, 0, { mode: "transcribe", tgt_lang: "en" }),
];

const ids = (order: ProjectOrder, filter: ProjectFilter = NO_FILTER) => selectProjects(list, filter, order, "ru").map((p) => p.pid);

describe("the order the project list is read in", () => {
  it("reads by creation, a project without a creation time as the oldest", () => {
    expect(ids("newest")).toEqual(["b", "a", "c", "d"]);
    expect(ids("oldest")).toEqual(["d", "c", "a", "b"]);
  });

  it("reads the recently edited first", () => {
    expect(ids("edited")).toEqual(["c", "a", "b", "d"]);
  });

  it("reads names as the language sorts them, numbers as numbers", () => {
    expect(ids("nameAsc")).toEqual(["b", "d", "c", "a"]);
    expect(ids("nameDesc")).toEqual(["a", "c", "d", "b"]);
  });

  it("reads by length", () => {
    expect(ids("longest")).toEqual(["c", "a", "b", "d"]);
    expect(ids("shortest")).toEqual(["d", "b", "a", "c"]);
  });
});

describe("search and filters", () => {
  it("finds by name regardless of case, and by language and mode", () => {
    expect(ids("newest", { ...NO_FILTER, query: "БАС" })).toEqual(["c", "d"]);
    expect(ids("newest", { ...NO_FILTER, query: "en" })).toEqual(["b", "d"]);
    expect(ids("newest", { ...NO_FILTER, query: "voiceover" })).toEqual(["c"]);
  });

  it("combines status, mode and language filters", () => {
    expect(ids("newest", { ...NO_FILTER, status: "done" })).toEqual(["b"]);
    expect(ids("newest", { ...NO_FILTER, status: "pending" })).toEqual(["a", "c", "d"]);
    expect(ids("newest", { ...NO_FILTER, modes: new Set(["dub", "transcribe"]), langs: new Set(["en"]) })).toEqual(["b", "d"]);
  });

  it("counts facet values, the most frequent first", () => {
    expect(facet(list, "tgt_lang")).toEqual([{ value: "en", count: 2 }, { value: "ru", count: 2 }]);
    expect(facet(list, "mode")).toEqual([{ value: "dub", count: 2 }, { value: "transcribe", count: 1 }, { value: "voiceover", count: 1 }]);
  });
});
