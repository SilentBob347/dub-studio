import { useTranslation } from "react-i18next";
import { ListFilter, Loader2, Scissors } from "lucide-react";
import type { Segment } from "../lib/api";
import { fitOver } from "../lib/fit";

// Шапка списка реплик: сколько не влезает в слот, фильтр «только не влезающие», «Сократить все».
export default function FitToolbar({ segments, onlyOver, onToggle, onShortenAll, busy, disabled }: {
  segments: Segment[];
  onlyOver: boolean;
  onToggle: () => void;
  onShortenAll: () => void;
  busy: boolean;
  disabled: boolean;
}) {
  const { t } = useTranslation();
  if (!segments.some((s) => s.fit)) return null;
  const over = segments.filter((s) => fitOver(s.fit)).length;
  const chip = "inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-[11px] border transition-colors disabled:opacity-40";
  return (
    <div className="flex flex-wrap items-center gap-1">
      <span className={`text-[11px] mr-0.5 ${over ? "text-[#ef4444]" : "text-[var(--color-muted)]"}`}>{t("fit.overCount", { count: over })}</span>
      <button onClick={onToggle} aria-pressed={onlyOver} disabled={!over && !onlyOver} title={t("fit.onlyOverTip")}
        className={`${chip} ${onlyOver ? "border-[var(--color-accent)] text-[var(--color-accent)] bg-[color-mix(in_oklab,var(--color-accent)_10%,transparent)]" : "border-[var(--color-border)] bg-[var(--color-surface-2)] text-[var(--color-muted)] hover:text-[var(--color-text)] hover:border-[var(--color-accent)]"}`}>
        <ListFilter size={12} />{t("fit.onlyOver")}
      </button>
      <button onClick={onShortenAll} disabled={disabled || busy || !over} title={t("fit.shortenAllTip")}
        className={`${chip} border-[var(--color-border)] bg-[var(--color-surface-2)] text-[var(--color-muted)] hover:text-[var(--color-accent)] hover:border-[var(--color-accent)]`}>
        {busy ? <Loader2 size={12} className="animate-spin" /> : <Scissors size={12} />}{t("fit.shortenAll")}
      </button>
    </div>
  );
}
