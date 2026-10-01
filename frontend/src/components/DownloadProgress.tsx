import { useTranslation } from "react-i18next";
import { Loader2, Pause, Play } from "lucide-react";
import type { DownloadJob } from "../lib/api";
import { fmtBytes } from "../lib/format";
import { useDownloadErrorText } from "../lib/setupText";

// Фоновая закачка: общий прогресс, скорость, ожидание сервера и кнопки паузы / продолжения.
export default function DownloadProgress({ job, onPause, onResume }: {
  job: DownloadJob;
  onPause: () => void;
  onResume: (ids: string[]) => void;
}) {
  const { t } = useTranslation();
  const errText = useDownloadErrorText();
  const running = job.status === "downloading";
  const pct = job.total > 0 ? Math.min(100, (job.downloaded / job.total) * 100) : 0;
  const phaseText: Record<string, string> = {
    waiting: t("downloads.phaseWaiting"),
    verify: t("downloads.phaseVerify"),
    download: t("downloads.phaseDownload"),
    extract: t("downloads.phaseExtract"),
    "": t("downloads.phaseStarting"),
  };
  const title = running
    ? phaseText[job.phase] ?? t("downloads.phaseDownload")
    : job.status === "paused" ? t("downloads.paused")
    : job.status === "interrupted" ? t("downloads.interrupted")
    : job.status === "failed" ? errText(job.errorCode)
    : t("downloads.completed");
  const tone = job.status === "failed" ? "text-[var(--color-warn)]" : running ? "text-[var(--color-text)]" : "text-[var(--color-muted)]";
  return (
    <div className="rounded-xl border border-[var(--color-border)] bg-[var(--color-surface)] px-3.5 py-3" aria-live="polite">
      <div className="flex items-center gap-2">
        {running && <Loader2 size={14} className="animate-spin text-[var(--color-accent)] shrink-0" />}
        <span className={`text-[13px] font-medium flex-1 min-w-0 truncate ${tone}`}>{title}</span>
        {running ? (
          <button onClick={onPause}
            className="shrink-0 inline-flex items-center gap-1 px-2.5 py-1 rounded-md border border-[var(--color-border)] text-[12px] text-[var(--color-muted)] hover:text-[var(--color-text)]">
            <Pause size={12} />{t("downloads.pause")}
          </button>
        ) : job.status !== "completed" && (
          <button onClick={() => onResume(job.ids)}
            className="shrink-0 inline-flex items-center gap-1 px-2.5 py-1 rounded-md bg-[var(--color-accent)] text-[var(--color-on-accent)] text-[12px] font-semibold hover:brightness-105">
            <Play size={12} />{t("downloads.resume")}
          </button>
        )}
      </div>
      {job.total > 0 && job.status !== "completed" && (
        <div className="mt-2 h-1.5 rounded-full bg-[var(--color-surface-2)] overflow-hidden" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(pct)}>
          <div className="h-full bg-[var(--color-accent)] transition-[width] duration-300" style={{ width: `${pct}%` }} />
        </div>
      )}
      <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-0.5 mono text-[11px] text-[var(--color-muted)]">
        {job.total > 0 && <span>{t("downloads.progress", { done: fmtBytes(job.downloaded), total: fmtBytes(job.total) })}</span>}
        {running && job.speedBps > 0 && <span>{t("downloads.speed", { speed: fmtBytes(job.speedBps) })}</span>}
        {running && job.waitingS > 0 && <span className="text-[var(--color-warn)]">{t("downloads.waitingServer", { s: job.waitingS })}</span>}
      </div>
      {job.status === "failed" && job.error && <div className="mt-1 mono text-[10.5px] text-[var(--color-muted)] break-words">{job.error}</div>}
    </div>
  );
}
