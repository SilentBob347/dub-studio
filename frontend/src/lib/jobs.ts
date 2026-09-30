// Слежение за джобами экрана анализа: прогресс в общий статус, отметки «из кэша», отмена и
// продолжение с места остановки (POST /projects/{pid}/resume ставит ту же джобу на тот же проект).
import { api, JobCancelledError, type JobEvent } from "./api";
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
export async function watchTracked(pid: string, kind: string, jobId: string, fallbackStage = ""): Promise<unknown> {
  const s = useStore.getState();
  s.setCurrentJob({ id: jobId, kind, pid });
  try {
    return await api.watchJob(jobId, (e) => trackJobEvent(e, fallbackStage));
  } finally {
    s.setCurrentJob(null);
    s.setQueuedAhead(null);
  }
}

// Можно ли продолжить последнюю джобу проекта (job.json).
export const RESUMABLE_STATES: ReadonlySet<string> = new Set(["failed", "interrupted", "cancelled"]);

// «Продолжить» последнюю джобу проекта. Анализ идёт на экране анализа (готовые этапы отмечены «из
// кэша»), затем `openEditor`; остальные джобы ставятся заново и `openEditor` сразу открывает редактор,
// где полоса джоб проекта подписывается на поставленную джобу. Ошибка постановки уходит вызывающему.
export async function continueProject(
  pid: string,
  kind: string,
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
    await watchWithResume(pid, "analyze", job_id);
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
export async function watchWithResume(pid: string, kind: string, jobId: string, fallbackStage = ""): Promise<unknown> {
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
