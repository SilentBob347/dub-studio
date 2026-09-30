import { create } from "zustand";
import type { JobKind, Project } from "./lib/api";

type Stage = "boot" | "offline" | "setup" | "empty" | "analyzing" | "editor" | "batch" | "multilang";
export type ExportItem = { id: string; name: string; status: "rendering" | "done" | "error"; msg: string; url?: string; pid?: string };
export type Activity = { t: number; text: string; kind: "work" | "done" | "error" };   // строка лога «что делает приложение»
export type CurrentJob = { id: string; kind: JobKind; pid: string };
// Джоба, за которой следит сам редактор (экспорт, озвучка, ремикс): полоса джоб проекта показывает её с «Отменить».
export type LocalJob = { id: string; kind: JobKind; pid: string; stage: string; msg: string; ahead: number | null };
// Джоба упала на экране анализа: ждём решения пользователя (продолжить с места остановки или назад).
export type JobFailure = { pid: string; msg: string; resolve: (choice: "continue" | "back") => void };

type State = {
  stage: Stage;
  pid: string | null;
  project: Project | null;
  progress: { stage: string; msg: string; pct: number | null };
  rendered: boolean;
  rendering: boolean;                // current project's export in flight (button state; does NOT block the screen)
  exports: ExportItem[];            // non-blocking export queue shown in the bottom-right Files panel
  activities: Activity[];           // журнал «что делает приложение» — статус-строка в шапке + разворот в лог
  past: Project[];                  // undo/redo history of Project snapshots
  future: Project[];
  rev: number;                       // preview cache-buster: bumped on every backend-confirmed frame change
  dubRev: number;                    // dub-audio cache-buster: bumped ONLY when the dub track is re-rendered (regen/export/finished job), NOT on every edit, so live edits don't reload <audio> mid-playback
  selBlur: number | null;            // selected blur-box index — SHARED between the left list and the canvas overlay
  selTitle: number | null;           // selected title index — SHARED between the left titles list and the canvas overlay
  justAnalyzed: boolean;             // только что прошёл analyze -> редактор один раз авто-генерит дуб (чтобы сразу слушать)
  setJustAnalyzed: (b: boolean) => void;
  audioOnly: boolean;                // вход без видео -> заголовок прогресса «Анализируем аудио», не «видео»
  setAudioOnly: (b: boolean) => void;
  jobSteps: string[] | null;         // ключи шагов степпера ТЕКУЩЕЙ джобы (по конфигу запуска); null = все
  setJobSteps: (s: string[] | null) => void;
  setStage: (s: Stage) => void;
  setPid: (p: string | null) => void;
  setProject: (p: Project | null) => void;
  setProgress: (stage: string, msg: string, pct?: number | null) => void;
  setRendered: (b: boolean) => void;
  setRendering: (b: boolean) => void;
  addExport: (e: ExportItem) => void;
  updateExport: (id: string, patch: Partial<ExportItem>) => void;
  pushHistory: (p: Project) => void; // snapshot the project BEFORE a mutation (for undo)
  undo: () => Project | null;        // returns the project to restore (PUT it) or null
  redo: () => Project | null;
  bump: () => void;
  bumpDub: () => void;                  // invalidate the rendered preview frame -> <img> refetches
  setSelBlur: (i: number | null) => void;
  setSelTitle: (i: number | null) => void;
  pushActivity: (text: string, kind?: Activity["kind"]) => void;   // добавить строку в журнал
  currentJob: CurrentJob | null;     // джоба экрана анализа (для «Отменить»)
  setCurrentJob: (j: CurrentJob | null) => void;
  queuedAhead: number | null;        // сколько джоб впереди, пока текущая в очереди
  setQueuedAhead: (n: number | null) => void;
  resumedStages: string[];           // стадии, взятые из кэша прошлого прогона (отметка в степпере)
  markResumed: (stage: string) => void;
  clearResumed: () => void;
  jobFailure: JobFailure | null;
  setJobFailure: (f: JobFailure | null) => void;
  localJobs: LocalJob[];
  putLocalJob: (j: LocalJob) => void;
  patchLocalJob: (id: string, patch: Partial<LocalJob>) => void;
  dropLocalJob: (id: string) => void;
  jobsRev: number;                   // растёт на каждом конце джобы редактора: полоса джоб перечитывает итог проекта
};

export const useStore = create<State>((set, get) => ({
  stage: "boot",   // при загрузке SPA сперва проверяем /setup/status; если чего-то обязательного нет -> "setup"
  pid: null,
  project: null,
  progress: { stage: "", msg: "", pct: null },
  rendered: false,
  rendering: false,
  exports: [],
  activities: [],
  past: [],
  future: [],
  rev: 0,
  dubRev: 0,
  selBlur: null,
  selTitle: null,
  justAnalyzed: false,
  setJustAnalyzed: (justAnalyzed) => set({ justAnalyzed }),
  audioOnly: false,
  setAudioOnly: (audioOnly) => set({ audioOnly }),
  jobSteps: null,
  setJobSteps: (jobSteps) => set({ jobSteps }),
  setStage: (stage) => set({ stage }),
  setPid: (pid) => set({ pid }),
  setProject: (project) => set({ project }),
  setProgress: (stage, msg, pct = null) => set((s) => {   // keep the last message on a stage-only tick; pct only during a download
    const progress = { stage, msg: msg || s.progress.msg, pct: pct ?? null };
    const text = (msg || stage || "").trim();
    if (!text) return { progress };
    const kind: Activity["kind"] = stage === "error" ? "error" : stage === "done" ? "done" : "work";
    const last = s.activities[s.activities.length - 1];
    if (last && last.text === text && last.kind === kind) return { progress };   // дедуп повторов
    return { progress, activities: [...s.activities, { t: Date.now(), text, kind }].slice(-200) };
  }),
  setRendered: (rendered) => set({ rendered }),
  setRendering: (rendering) => set({ rendering }),
  addExport: (e) => set((s) => ({ exports: [e, ...s.exports.filter((x) => x.id !== e.id)] })),   // дедуп по id: повторный экспорт того же проекта заменяет запись, а не плодит дубли
  updateExport: (id, patch) => set((s) => ({ exports: s.exports.map((x) => (x.id === id ? { ...x, ...patch } : x)) })),
  pushHistory: (p) => set((s) => {
    // no-op if the snapshot matches the current head: callers may re-snapshot the same
    // baseline (e.g. a second keystroke in the same edit burst), and an unconditional
    // future:[] there would silently kill the redo stack.
    const head = s.past[s.past.length - 1];
    if (head && JSON.stringify(head) === JSON.stringify(p)) return {};
    return { past: [...s.past, p].slice(-60), future: [] };
  }),
  undo: () => {
    const s = get(); if (!s.past.length || !s.project) return null;
    const prev = s.past[s.past.length - 1];
    set({ past: s.past.slice(0, -1), future: [s.project, ...s.future], project: prev });
    return prev;
  },
  redo: () => {
    const s = get(); if (!s.future.length || !s.project) return null;
    const next = s.future[0];
    set({ future: s.future.slice(1), past: [...s.past, s.project], project: next });
    return next;
  },
  bump: () => set((s) => ({ rev: s.rev + 1 })),
  bumpDub: () => set({ dubRev: Date.now() }),
  setSelBlur: (selBlur) => set({ selBlur }),
  setSelTitle: (selTitle) => set({ selTitle }),
  pushActivity: (text, kind = "work") => set((s) => {
    const clean = (text || "").trim();
    if (!clean) return {};
    const last = s.activities[s.activities.length - 1];
    if (last && last.text === clean && last.kind === kind) return {};   // дедуп повторов
    return { activities: [...s.activities, { t: Date.now(), text: clean, kind }].slice(-200) };
  }),
  currentJob: null,
  setCurrentJob: (currentJob) => set({ currentJob }),
  queuedAhead: null,
  setQueuedAhead: (queuedAhead) => set({ queuedAhead }),
  resumedStages: [],
  markResumed: (stage) => set((s) => (s.resumedStages.includes(stage) ? {} : { resumedStages: [...s.resumedStages, stage] })),
  clearResumed: () => set({ resumedStages: [] }),
  jobFailure: null,
  setJobFailure: (jobFailure) => set({ jobFailure }),
  localJobs: [],
  putLocalJob: (j) => set((s) => ({ localJobs: [...s.localJobs.filter((x) => x.id !== j.id), j] })),
  patchLocalJob: (id, patch) => set((s) => ({ localJobs: s.localJobs.map((x) => (x.id === id ? { ...x, ...patch } : x)) })),
  dropLocalJob: (id) => set((s) => ({ localJobs: s.localJobs.filter((x) => x.id !== id), jobsRev: s.jobsRev + 1 })),
  jobsRev: 0,
}));
