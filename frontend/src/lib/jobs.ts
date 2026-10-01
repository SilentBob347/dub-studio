// Слежение за джобами экрана анализа: прогресс в общий статус, отметки «из кэша», отмена и
// продолжение с места остановки (POST /projects/{pid}/resume ставит ту же джобу на тот же проект).
import { api, JobCancelledError, JobConflictError, type AnalyzeResult, type JobEvent, type JobKind, type JobState, type VoiceSlotsOutcome } from "./api";
import i18n from "./i18n";
import { useStore } from "../store";

// Событие джобы -> статус-строка/степпер. `fallbackStage` — стадия, если событие её не несёт.
export function trackJobEvent(e: JobEvent, fallbackStage = ""): void {
  const s = useStore.getState();
  if (e.type === "queued") {
    s.setQueuedAhead(e.position ?? null);
    return;
  }
  if (e.type === "running") {
    s.setQueuedAhead(null);
    return;
  }
  if (e.type !== "progress") return;
  s.setQueuedAhead(null);
  s.setProgress(e.stage || fallbackStage, e.msg || "", e.pct ?? null);
  if (e.resumed && e.stage) s.markResumed(e.stage);
}

// Следить за джобой, пока она текущая на экране анализа (кнопка «Отменить» видит её id).
export async function watchTracked(pid: string, kind: JobKind, jobId: string, fallbackStage = ""): Promise<unknown> {
  const s = useStore.getState();
  s.setCurrentJob({ id: jobId, kind, pid });
  try {
    return await api.watchJob(jobId, (e) => trackJobEvent(e, fallbackStage));
  } finally {
    s.setCurrentJob(null);
    s.setQueuedAhead(null);
  }
}

// Джобы, за которыми следит сам редактор. Полоса джоб проекта их не подхватывает: итог обрабатывает
// тот, кто их поставил, а id остаётся здесь и после конца, чтобы подписка полосы не обработала его второй раз.
const ownJobIds = new Set<string>();
export const isOwnJob = (jobId: string): boolean => ownJobIds.has(jobId);

// Следить за джобой, поставленной из редактора: пока она идёт, полоса джоб проекта показывает её этап и «Отменить».
export async function watchLocal(pid: string, kind: JobKind, jobId: string, onEvent: (e: JobEvent) => void): Promise<unknown> {
  ownJobIds.add(jobId);
  useStore.getState().putLocalJob({ id: jobId, kind, pid, stage: "", msg: "", ahead: null });
  try {
    return await api.watchJob(jobId, (e) => {
      const s = useStore.getState();
      if (e.type === "queued") s.patchLocalJob(jobId, { ahead: e.position ?? null });
      else if (e.type === "running") s.patchLocalJob(jobId, { ahead: null });
      else if (e.type === "progress") s.patchLocalJob(jobId, { stage: e.stage ?? "", msg: e.msg ?? "", ahead: null });
      onEvent(e);
    });
  } finally {
    useStore.getState().dropLocalJob(jobId);
  }
}

// Можно ли продолжить последнюю джобу проекта (job.json).
export const RESUMABLE_STATES: ReadonlySet<JobState> = new Set<JobState>(["failed", "interrupted", "cancelled"]);

// «Продолжить» последнюю джобу проекта. Анализ идёт на экране анализа (готовые этапы отмечены «из
// кэша»), затем `openEditor`; остальные джобы ставятся заново и `openEditor` сразу открывает редактор,
// где полоса джоб проекта подписывается на поставленную джобу. Ошибка постановки уходит вызывающему.
export async function continueProject(
  pid: string,
  kind: JobKind,
  audioOnly: boolean,
  openEditor: (pid: string) => Promise<void> | void,
): Promise<void> {
  if (kind !== "analyze") {
    await api.resumeProject(pid);
    await openEditor(pid);
    return;
  }
  const s = useStore.getState();
  s.setAudioOnly(audioOnly);
  s.setJobSteps(null);
  s.setProgress("", "", null);
  s.clearResumed();
  s.setStage("analyzing");
  try {
    const { job_id } = await api.resumeProject(pid);
    await finishAnalyze(pid, await watchWithResume(pid, "analyze", job_id));
  } catch (err) {
    s.setStage("empty");
    if (!(err instanceof JobCancelledError)) s.setProgress("error", err instanceof Error ? err.message : String(err), null);
    return;
  }
  await openEditor(pid);
}

// Как watchTracked, но ошибка джобы не сбрасывает экран: пользователь выбирает «Продолжить» (та же
// джоба заново с готовыми этапами из кэша) или «Назад» (ошибка уходит вызывающему). Отмена — сразу
// JobCancelledError.
export async function watchWithResume(pid: string, kind: JobKind, jobId: string, fallbackStage = ""): Promise<unknown> {
  let id = jobId;
  for (;;) {
    try {
      return await watchTracked(pid, kind, id, fallbackStage);
    } catch (err) {
      if (err instanceof JobCancelledError) throw err;
      const choice = await new Promise<"continue" | "back">((resolve) =>
        useStore.getState().setJobFailure({ pid, msg: err instanceof Error ? err.message : String(err), resolve }));
      useStore.getState().setJobFailure(null);
      if (choice === "back") throw err;
      id = (await api.resumeProject(pid)).job_id;
    }
  }
}

// Конец анализа в окне: журнал по голосам из библиотеки (их раздал сервер в конце анализа), свежий проект
// и для дубляжа/закадра — озвучка на экране анализа, чтобы редактор открылся с готовым дубом. Общий путь
// первого запуска и «Продолжить», поэтому проект после продолжения тот же, что после первого запуска.
export async function finishAnalyze(pid: string, result: unknown): Promise<void> {
  const s = useStore.getState();
  reportVoiceSlots((result as AnalyzeResult).post.voice_slots);
  const project = await api.getProject(pid);
  s.setProject(project);
  if (project.mode !== "dub" && project.mode !== "voiceover") return;
  try {
    const { job_id } = await api.render(pid);
    await watchTracked(pid, "render", job_id, "voicing");
    s.setProject(await api.getProject(pid));
    s.bumpDub();
  } catch (err) {
    if (err instanceof JobCancelledError) return;   // озвучку отменили: редактор откроется на покадровом превью
    s.pushActivity(err instanceof Error ? err.message : String(err), "error");
  }
}

// Итог раздачи голосов из библиотеки -> журнал. undefined — раздачу не просили.
export function reportVoiceSlots(o: VoiceSlotsOutcome | undefined): void {
  if (!o) return;
  const s = useStore.getState();
  if ("assigned" in o) s.pushActivity(i18n.t("voiceSlots.assigned", { count: o.assigned }), "done");
  else if (o.error === "missing_voices") s.pushActivity(i18n.t("voiceSlots.missing", { names: o.names.join(", ") }), "error");
  else s.pushActivity(i18n.t("voiceSlots.noVocals"), "error");
}

// Поставить джобу проекта. Если по проекту уже идёт джоба того же класса (409), дождаться её конца и
// поставить снова: джобы одного проекта идут друг за другом. `onWait` получает вид джобы, которую ждём.
export async function enqueueWhenFree(start: () => Promise<{ job_id: string }>, onWait: (kind: JobKind) => void): Promise<{ job_id: string }> {
  for (;;) {
    try {
      return await start();
    } catch (err) {
      if (!(err instanceof JobConflictError)) throw err;
      onWait(err.kind);
      await waitJobEnd(err.jobId);
    }
  }
}

// Ждать терминала джобы long-poll'ом; джобы нет в истории — она давно завершилась.
async function waitJobEnd(jobId: string): Promise<void> {
  for (;;) {
    const snap = await api.waitJob(jobId, 55);
    if (!snap || snap.status === "done" || snap.status === "error" || snap.status === "cancelled") return;
  }
}
