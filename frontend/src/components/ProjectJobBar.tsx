import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2, RotateCw, Square, X } from "lucide-react";
import { api, JobCancelledError, type JobSnapshot, type ProjectJob } from "../lib/api";
import { continueProject, RESUMABLE_STATES } from "../lib/jobs";
import { useJobErrorText, useJobStateLabel } from "../lib/jobLabels";
import { STAGE_TO_STEPKEY } from "../lib/stages";
import { useStore } from "../store";

type Live = { stage: string; msg: string; ahead: number | null };

// Джобы открытого проекта: при открытии подписывается на уже идущую (после перезагрузки окна, после
// «Продолжить» в «Недавних», поставленную агентом) и показывает её прогресс с «Отменить»; если последняя
// джоба проекта прервана/упала/отменена — предлагает «Продолжить» с места остановки.
export default function ProjectJobBar({ pid }: { pid: string }) {
  const { t } = useTranslation();
  const stateLabel = useJobStateLabel();
  const errText = useJobErrorText();
  const [job, setJob] = useState<JobSnapshot | null>(null);
  const [live, setLive] = useState<Live | null>(null);
  const [record, setRecord] = useState<ProjectJob | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [hidden, setHidden] = useState(false);
  const [reload, setReload] = useState(0);

  useEffect(() => {
    let alive = true;
    api.jobs(pid)
      .then(async (r) => {
        if (!alive) return;
        const active = r.jobs.find((j) => j.status === "queued" || j.status === "running") ?? null;
        setJob(active);
        setRecord(active ? null : r.project_job);
        if (!active) return;
        setLive({ stage: active.stage ?? "", msg: active.msg ?? "", ahead: active.position ?? null });
        try {
          await api.watchJob(active.id, (e) => {
            if (!alive) return;
            if (e.type === "queued") setLive((l) => ({ stage: l?.stage ?? "", msg: l?.msg ?? "", ahead: e.position ?? null }));
            else if (e.type === "progress") setLive({ stage: e.stage ?? "", msg: e.msg ?? "", ahead: null });
          });
          if (!alive) return;
          const s = useStore.getState();
          s.setProject(await api.getProject(pid));
          s.setRendered(false);
          s.bump();
        } catch (e) {
          if (alive && !(e instanceof JobCancelledError)) setError(e instanceof Error ? e.message : String(e));
        } finally {
          if (alive) {
            setJob(null);
            setLive(null);
            setBusy(false);
            setReload((n) => n + 1);
          }
        }
      })
      .catch((e) => { if (alive) setError(e instanceof Error ? e.message : String(e)); });
    return () => { alive = false; };
  }, [pid, reload]);

  async function cancel(id: string) {
    setBusy(true);
    try {
      await api.cancelJob(id);
    } catch (e) {
      setBusy(false);
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function resume(rec: ProjectJob) {
    setBusy(true);
    setError(null);
    const s = useStore.getState();
    const meta = s.project?.meta;
    const audioOnly = !!meta && (meta.width <= 0 || meta.height <= 0);
    try {
      await continueProject(pid, rec.kind, audioOnly, async (id) => {
        if (rec.kind === "analyze") {
          s.setProject(await api.getProject(id));
          s.setStage("editor");
        }
        setReload((n) => n + 1);
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  if (hidden) return null;
  const kindLabel = (kind: string) => t(`jobs.kind.${kind}`);
  const box = "fixed top-14 left-1/2 -translate-x-1/2 z-40 max-w-[min(640px,90vw)] flex items-center gap-2.5 rounded-lg border px-3 py-1.5 text-[12px] shadow-lg backdrop-blur";

  if (job) {
    const step = live?.stage ? STAGE_TO_STEPKEY[live.stage] : undefined;
    const detail = live?.ahead != null ? t("jobs.queuedAhead", { n: live.ahead }) : step ? t(`analyze.${step}`) : live?.msg ?? "";
    return (
      <div className={`${box} border-[var(--color-border)] bg-[var(--color-surface)]/95`}>
        <Loader2 size={13} className="animate-spin text-[var(--color-accent)] shrink-0" />
        <span className="font-medium shrink-0">{kindLabel(job.kind)}</span>
        <span className="truncate text-[var(--color-muted)]">{detail}</span>
        <button onClick={() => cancel(job.id)} disabled={busy}
          className="ml-1 shrink-0 inline-flex items-center gap-1 px-2 py-0.5 rounded-md border border-[var(--color-border)] text-[var(--color-muted)] hover:text-[var(--color-text)] hover:border-[var(--color-warn)] disabled:opacity-50 transition-colors">
          {busy ? <Loader2 size={12} className="animate-spin" /> : <Square size={11} />}{busy ? t("jobs.cancelling") : t("jobs.cancel")}
        </button>
      </div>
    );
  }

  if (record && RESUMABLE_STATES.has(record.state)) {
    return (
      <div className={`${box} border-[var(--color-warn)]/40 bg-[color-mix(in_oklab,var(--color-warn)_12%,var(--color-surface))]`}>
        <span className="font-medium shrink-0">{kindLabel(record.kind)}</span>
        <span className="truncate text-[var(--color-warn)]" title={errText(record.error)}>{stateLabel(record.state, record.stage)}</span>
        {error && <span className="truncate mono text-[11px] text-[var(--color-warn)]" title={error}>{error}</span>}
        <button onClick={() => resume(record)} disabled={busy} title={t("jobs.continueHint")}
          className="ml-1 shrink-0 inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-[var(--color-accent)] text-[var(--color-on-accent)] font-semibold hover:opacity-90 disabled:opacity-50 transition-opacity">
          {busy ? <Loader2 size={12} className="animate-spin" /> : <RotateCw size={12} />}{t("jobs.continue")}
        </button>
        <button onClick={() => setHidden(true)} title={t("jobs.dismiss")}
          className="shrink-0 text-[var(--color-muted)] hover:text-[var(--color-text)]"><X size={14} /></button>
      </div>
    );
  }

  if (error) {
    return (
      <div className={`${box} border-[var(--color-warn)]/40 bg-[var(--color-surface)]/95`}>
        <span className="truncate mono text-[11px] text-[var(--color-warn)]" title={error}>{error}</span>
        <button onClick={() => setHidden(true)} title={t("jobs.dismiss")}
          className="shrink-0 text-[var(--color-muted)] hover:text-[var(--color-text)]"><X size={14} /></button>
      </div>
    );
  }
  return null;
}
