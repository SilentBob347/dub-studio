import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowLeft, ChevronDown, RotateCw } from "lucide-react";
import { useStore } from "../store";

// Ошибка джобы на экране анализа: вместо сброса на стартовый экран — «Продолжить» (та же джоба на том
// же проекте, готовые этапы из кэша) или «Назад». Решение ждёт watchWithResume.
export default function JobFailurePanel() {
  const { t } = useTranslation();
  const failure = useStore((s) => s.jobFailure);
  const [details, setDetails] = useState(false);
  if (!failure) return null;
  return (
    <div className="mt-6 rounded-lg border border-[var(--color-warn)]/40 bg-[color-mix(in_oklab,var(--color-warn)_10%,transparent)] px-3 py-2.5">
      <div className="text-[13px] font-semibold text-[var(--color-warn)]">{t("jobs.failedTitle")}</div>
      <div className="mt-1 text-[12px] text-[var(--color-muted)]">{t("jobs.failedHint")}</div>
      <button onClick={() => setDetails((d) => !d)}
        className="mt-1.5 inline-flex items-center gap-1 text-[11px] text-[var(--color-muted)] hover:text-[var(--color-text)]">
        <ChevronDown size={12} className={details ? "rotate-180 transition-transform" : "transition-transform"} />{t("jobs.details")}
      </button>
      {details && <div className="mt-1 mono text-[11px] break-words text-[var(--color-text)]">{failure.msg}</div>}
      <div className="mt-3 flex justify-end gap-2">
        <button onClick={() => failure.resolve("back")}
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] text-[13px] text-[var(--color-muted)] hover:text-[var(--color-text)] transition-colors">
          <ArrowLeft size={13} />{t("jobs.back")}
        </button>
        <button onClick={() => failure.resolve("continue")}
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-[13px] font-semibold hover:opacity-90 transition-opacity">
          <RotateCw size={13} />{t("jobs.continue")}
        </button>
      </div>
    </div>
  );
}
