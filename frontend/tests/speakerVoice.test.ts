import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import i18n from "../src/lib/i18n";
import { makeSpeakerVoice } from "../src/lib/speakerVoice";
import { useStore } from "../src/store";

const answer = (status: number, body: unknown) =>
  vi.fn(async () => new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } }));

const lastActivity = () => useStore.getState().activities.at(-1);

beforeAll(async () => {
  await i18n.changeLanguage("en");
});

afterEach(() => {
  vi.unstubAllGlobals();
  useStore.setState({ activities: [] });
});

describe("making a voice from a speaker", () => {
  it("shows why when the project has no vocals and no separation engine", async () => {
    const fetch = answer(409, { error: "no_separation", detail: "D:/app/tools/bsroformer/bs_roformer-cli.exe" });
    vi.stubGlobal("fetch", fetch);
    await expect(makeSpeakerVoice("p1", "1", "Speaker 1")).resolves.toBeNull();
    const [url, init] = fetch.mock.calls[0] as unknown as [string, RequestInit];
    expect(url.endsWith("/projects/p1/speaker-voice")).toBe(true);
    expect(init.method).toBe("POST");
    expect(lastActivity()).toMatchObject({
      kind: "error",
      text: "Couldn't make a voice from speaker 1: this project has no separated vocals and the vocal separation engine isn't installed, so the voice would carry the music. Install it in Settings → Models → Voice cleanup · separation.",
    });
  });

  it("shows the engine's reason when separating the line fails", async () => {
    vi.stubGlobal("fetch", answer(500, { error: "separation_failed", detail: "движок завершился с ошибкой: код Some(1)" }));
    await expect(makeSpeakerVoice("p1", "0", "Speaker 0")).resolves.toBeNull();
    expect(lastActivity()).toMatchObject({
      kind: "error",
      text: "Couldn't make a voice from speaker 0: separating the voice from the music failed — движок завершился с ошибкой: код Some(1)",
    });
  });

  it("shows the error when the studio does not answer", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => { throw new TypeError("Failed to fetch"); }));
    await expect(makeSpeakerVoice("p1", "2", "Speaker 2")).resolves.toBeNull();
    expect(lastActivity()).toMatchObject({ kind: "error", text: "Couldn't make a voice from speaker 2: Failed to fetch" });
  });

  it("follows the window's language", async () => {
    await i18n.changeLanguage("ru");
    try {
      vi.stubGlobal("fetch", answer(400, { error: "no_speaker_lines", detail: "3" }));
      await expect(makeSpeakerVoice("p1", "3", "Спикер 3")).resolves.toBeNull();
      expect(lastActivity()).toMatchObject({ kind: "error", text: "У спикера 3 нет реплик, из которых можно сделать голос" });
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("returns the made voice and logs no error", async () => {
    vi.stubGlobal("fetch", answer(200, { ok: true, name: "Speaker 1", voices: ["Speaker 1"] }));
    await expect(makeSpeakerVoice("p1", "1", "Speaker 1")).resolves.toEqual({ ok: true, name: "Speaker 1", voices: ["Speaker 1"] });
    expect(useStore.getState().activities).toEqual([]);
  });
});
