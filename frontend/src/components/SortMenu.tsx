import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowUpDown, Check } from "lucide-react";
import type { ProjectOrder } from "../lib/projectList";

const LABELS = {
  newest: "projects.sort.newest",
  oldest: "projects.sort.oldest",
  edited: "projects.sort.edited",
  nameAsc: "projects.sort.nameAsc",
  nameDesc: "projects.sort.nameDesc",
  longest: "projects.sort.longest",
  shortest: "projects.sort.shortest",
} as const satisfies Record<ProjectOrder, string>;

// «Сортировка: сначала новые» — открывает порядки, в которых можно читать список.
export default function SortMenu({ order, orders, onChange }: { order: ProjectOrder; orders: readonly ProjectOrder[]; onChange: (order: ProjectOrder) => void }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const menu = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const outside = (e: MouseEvent) => { if (menu.current && !menu.current.contains(e.target as Node)) setOpen(false); };
    const escape = (e: KeyboardEvent) => { if (e.key === "Escape") { e.stopPropagation(); setOpen(false); } };
    document.addEventListener("mousedown", outside);
    window.addEventListener("keydown", escape, true);
    return () => {
      document.removeEventListener("mousedown", outside);
      window.removeEventListener("keydown", escape, true);
    };
  }, [open]);

  const current = t(LABELS[order]);
  return (
    <div className="relative" ref={menu}>
      <button type="button" onClick={() => setOpen((o) => !o)} aria-haspopup="menu" aria-expanded={open}
        aria-label={t("projects.sort.label", { order: current })} title={t("projects.sort.label", { order: current })}
        className={`inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg border text-[12px] font-medium transition-colors ${open ? "border-[var(--color-accent)] text-[var(--color-text)] bg-[var(--color-surface-2)]" : "border-[var(--color-border)] bg-[var(--color-surface-2)] text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
        <ArrowUpDown size={13} />
        <span className="hidden sm:inline whitespace-nowrap">{current}</span>
      </button>
      {open && (
        <div role="menu" className="absolute right-0 top-full z-50 mt-1.5 w-56 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] shadow-xl py-1">
          <div className="px-3 py-1.5 text-[10px] uppercase tracking-[0.12em] text-[var(--color-muted)]">{t("projects.sort.title")}</div>
          {orders.map((o) => (
            <button key={o} type="button" role="menuitemradio" aria-checked={o === order}
              onClick={() => { onChange(o); setOpen(false); }}
              className="w-full flex items-center justify-between gap-2 px-3 py-1.5 text-left text-[12px] text-[var(--color-text)] hover:bg-[var(--color-surface-2)] transition-colors">
              {t(LABELS[o])}
              {o === order && <Check size={13} className="text-[var(--color-accent)] shrink-0" />}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
