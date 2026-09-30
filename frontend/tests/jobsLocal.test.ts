import { describe, expect, it, vi } from "vitest";
import { JobCancelledError, type JobEvent } from "../src/lib/api";
import { isOwnJob, watchLocal } from "../src/lib/jobs";
import { useStore } from "../src/store";

// SSE stream of the job being watched: the test pushes events and ends it.
const stream = vi.hoisted(() => ({
  onEvent: null as ((e: JobEvent) => void) | null,
  settle: null as ((err?: Error) => void) | null,
}));

function emit(e: JobEvent) {
  if (!stream.onEvent) throw new Error("no job is being watched");
  stream.onEvent(e);
}

function finish(err?: Error) {
  if (!stream.settle) throw new Error("no job is being watched");
  stream.settle(err);
}

vi.mock("../src/lib/api", async (importOriginal) => {
  const mod = await importOriginal<typeof import("../src/lib/api")>();
  return {
    ...mod,
    api: {
      ...mod.api,
      watchJob: (_jobId: string, onEvent: (e: JobEvent) => void) =>
        new Promise<unknown>((resolve, reject) => {
          stream.onEvent = onEvent;
          stream.settle = (err) => (err ? reject(err) : resolve({ ok: true }));
        }),
    },
  };
});

describe("jobs started from the editor", () => {
  it("stay in the project job bar with their stage while running and leave it at the end", async () => {
    const rev = useStore.getState().jobsRev;
    const seen: string[] = [];
    const done = watchLocal("p1", "render", "j1", (e) => seen.push(e.type));
    expect(useStore.getState().localJobs).toEqual([{ id: "j1", kind: "render", pid: "p1", stage: "", msg: "", ahead: null }]);

    emit({ type: "queued", position: 2 });
    expect(useStore.getState().localJobs[0].ahead).toBe(2);
    emit({ type: "progress", stage: "tts", msg: "3/10" });
    expect(useStore.getState().localJobs[0]).toMatchObject({ stage: "tts", msg: "3/10", ahead: null });

    finish();
    await expect(done).resolves.toEqual({ ok: true });
    expect(useStore.getState().localJobs).toEqual([]);
    expect(useStore.getState().jobsRev).toBe(rev + 1);
    expect(seen).toEqual(["queued", "progress"]);
    // The bar's own subscription must not handle the finished job a second time.
    expect(isOwnJob("j1")).toBe(true);
    expect(isOwnJob("other")).toBe(false);
  });

  it("reach the caller as a cancellation when cancelled from the bar", async () => {
    const done = watchLocal("p1", "dub_audio", "j2", () => {});
    expect(useStore.getState().localJobs.map((j) => j.id)).toEqual(["j2"]);
    finish(new JobCancelledError());
    await expect(done).rejects.toBeInstanceOf(JobCancelledError);
    expect(useStore.getState().localJobs).toEqual([]);
  });
});
