import type { LaunchDefaults, LaunchDefaultsState } from "./api";

// Ключи localStorage, в которых окно раньше держало дефолты запуска. Сервер хранит их сам
// (GET/PATCH /settings/launch); старые значения переносятся туда один раз.
export const LEGACY_KEYS = [
  "dub-casting", "dub-casting-ref", "dub-content-type", "dub-vo-gain", "dub-tr-style-choice", "dub-tr-style-custom",
  "dub-sub-blur", "dub-keep-orig", "dub-container", "dub-voice-src", "dub-voice-slots-m", "dub-voice-slots-f",
] as const;

type LegacyStore = Pick<Storage, "getItem" | "removeItem">;

export type LaunchClient = {
  load: () => Promise<LaunchDefaultsState>;
  save: (patch: Partial<LaunchDefaults>) => Promise<LaunchDefaultsState>;
};

const TR_STYLES = ["", "technical", "literary", "casual", "custom"] as const;
const CONTENT_TYPES = ["auto", "real", "anime"] as const;
const SLUG = /^[a-z0-9-]*$/;

const stringList = (raw: string): string[] => {
  let parsed: unknown;
  try { parsed = JSON.parse(raw); } catch { return []; }
  return Array.isArray(parsed) ? parsed.filter((x): x is string => typeof x === "string") : [];
};

// Старые значения -> правка для сервера. Каждое значение читается так же, как его читал стартовый экран
// до переноса, поэтому правка всегда допустима для сервера.
export function legacyPatch(store: LegacyStore): { patch: Partial<LaunchDefaults>; keys: string[] } {
  const patch: Partial<LaunchDefaults> = {};
  const keys: string[] = [];
  const read = (key: (typeof LEGACY_KEYS)[number], apply: (raw: string) => void) => {
    const raw = store.getItem(key);
    if (raw === null) return;
    apply(raw);
    keys.push(key);
  };
  read("dub-casting", (v) => { patch.casting = v === "1"; });
  read("dub-casting-ref", (v) => { patch.casting_ref = SLUG.test(v) ? v : ""; });
  read("dub-content-type", (v) => { patch.content_type = CONTENT_TYPES.find((c) => c === v) ?? "auto"; });
  read("dub-vo-gain", (v) => {
    const n = parseFloat(v);
    patch.vo_gain_db = Number.isFinite(n) ? Math.min(0, Math.max(-24, n)) : -12;
  });
  read("dub-tr-style-choice", (v) => { patch.tr_style = TR_STYLES.find((s) => s === v) ?? ""; });
  read("dub-tr-style-custom", (v) => { patch.tr_style_custom = v.slice(0, 4000); });
  read("dub-sub-blur", (v) => { patch.sub_blur = v !== "0"; });
  read("dub-keep-orig", (v) => { patch.keep_orig = v === "1"; });
  read("dub-container", (v) => { patch.container = v === "mkv" ? "mkv" : "mp4"; });
  read("dub-voice-src", (v) => { patch.voice_src = v === "library" ? "library" : "clone"; });
  read("dub-voice-slots-m", (v) => { patch.voice_slots_m = stringList(v).slice(0, 64); });
  read("dub-voice-slots-f", (v) => { patch.voice_slots_f = stringList(v).slice(0, 64); });
  return { patch, keys };
}

// Дефолты с сервера, с одноразовым переносом старого выбора окна. Сервер ещё ничего не хранит — старые
// значения уходят ему, и из localStorage удаляются только после того, как сервер их принял. Сервер уже
// хранит дефолты (перенесло другое окно или выбор сделан после переноса) — старые значения устарели.
// Сервер отверг перенос — ключи остаются, ошибка уходит вызывающему.
export async function loadWithMigration(client: LaunchClient, store: LegacyStore): Promise<LaunchDefaultsState> {
  const state = await client.load();
  const { patch, keys } = legacyPatch(store);
  if (keys.length === 0) return state;
  const next = state.saved ? state : await client.save(patch);
  for (const key of keys) store.removeItem(key);
  return next;
}

// Правки формы копятся и уходят одной PATCH после паузы; запросы идут строго по очереди, чтобы поздний
// ответ не перетёр более новый выбор. flush() — отправить накопленное сразу (уход со стартового экрана).
export function createLaunchSaver(save: LaunchClient["save"], onError: (e: unknown) => void, delayMs = 300) {
  let pending: Partial<LaunchDefaults> = {};
  let timer: ReturnType<typeof setTimeout> | null = null;
  let chain: Promise<void> = Promise.resolve();
  const send = () => {
    timer = null;
    const patch = pending;
    pending = {};
    if (Object.keys(patch).length === 0) return;
    chain = chain.then(() => save(patch).then(() => undefined, onError));
  };
  return {
    queue(patch: Partial<LaunchDefaults>) {
      pending = { ...pending, ...patch };
      if (timer) clearTimeout(timer);
      timer = setTimeout(send, delayMs);
    },
    flush() {
      if (timer) clearTimeout(timer);
      send();
      return chain;
    },
  };
}
