import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2, Square } from "lucide-react";
import { api } from "../lib/api";
import { useStore } from "../store";

// «Отменить» текущую джобу экрана анализа. Отмена кооперативная: сервер останавливает джобу между
// стадиями/сегментами и сразу убивает её дочерние процессы; готовые этапы остаются в кэше проекта.
export default function CancelJobButton() {
  const { t } = useTranslation();
  const job = useStore((s) => s.currentJob);
  const [pendingId, setPendingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  if (!job) return null;
  const cancelling = pendingId === job.id;
  async function cancel(id: string) {
    setPendingId(id);
    setError(null);
    try {
      await api.cancelJob(id);
    } catch (e) {
      setPendingId(null);
      setError(e instanceof Error ? e.message : String(e));
    }
  }
  return (
    <div className="mt-5 flex flex-col items-center gap-1">
      <button onClick={() => cancel(job.id)} disabled={cancelling}
        className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] text-[13px] text-[var(--color-muted)] hover:text-[var(--color-text)] hover:border-[var(--color-warn)] disabled:opacity-60 transition-colors">
        {cancelling ? <Loader2 size={13} className="animate-spin" /> : <Square size={12} />}
        {cancelling ? t("jobs.cancelling") : t("jobs.cancel")}
      </button>
      {error && <div className="mono text-[11px] text-[var(--color-warn)] break-words text-center">{error}</div>}
    </div>
  );
}
