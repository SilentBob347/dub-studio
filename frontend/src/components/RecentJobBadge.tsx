import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2, RotateCw } from "lucide-react";
import type { ProjectSummary } from "../lib/api";
import { continueProject, RESUMABLE_STATES } from "../lib/jobs";
import { useJobErrorText, useJobStateLabel } from "../lib/jobLabels";
import { useStore } from "../store";

// Бейдж последней джобы проекта в строке карточки «Недавних» (для незавершённых).
export function JobStateLabel({ p }: { p: ProjectSummary }) {
  const label = useJobStateLabel();
  const errText = useJobErrorText();
  if (!p.job_state || p.job_state === "done") return null;
  const warn = RESUMABLE_STATES.has(p.job_state);
  return (
    <span title={errText(p.job_error)}
      className={`min-w-0 truncate px-1.5 rounded text-[10px] font-medium ${warn ? "text-[var(--color-warn)] bg-[color-mix(in_oklab,var(--color-warn)_12%,transparent)]" : "text-[var(--color-accent)] bg-[color-mix(in_oklab,var(--color-accent)_12%,transparent)]"}`}>
      {label(p.job_state, p.job_stage)}
    </span>
  );
}

// «Продолжить» прерванную/упавшую/отменённую джобу проекта с места остановки.
export function ContinueJobButton({ p, onOpen }: { p: ProjectSummary; onOpen: (pid: string) => Promise<void> | void }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  if (!p.job_state || !p.job_kind || !RESUMABLE_STATES.has(p.job_state)) return null;
  const kind = p.job_kind;
  async function run(e: React.MouseEvent) {
    e.stopPropagation();
    setBusy(true);
    try {
      await continueProject(p.pid, kind, p.audio_only, onOpen);
    } catch (err) {
      useStore.getState().pushActivity(err instanceof Error ? err.message : String(err), "error");
    } finally {
      setBusy(false);
    }
  }
  return (
    <button onClick={run} disabled={busy} title={t("jobs.continueHint")}
      className="shrink-0 inline-flex items-center gap-1 mr-1 px-2 py-1 rounded-md text-[12px] font-medium text-[var(--color-accent)] hover:bg-white/5 disabled:opacity-50 transition">
      {busy ? <Loader2 size={13} className="animate-spin" /> : <RotateCw size={13} />}{t("jobs.continue")}
    </button>
  );
}
