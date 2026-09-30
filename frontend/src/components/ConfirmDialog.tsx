import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { TriangleAlert } from "lucide-react";
import { useStore } from "../store";

// «Точно?» перед необратимым действием (удалить проект, профиль кастинга). Спрашивает store.askConfirm; один на окно.
// Отмена — Esc, клик мимо и кнопка «Отмена» (в фокусе по умолчанию).
export default function ConfirmDialog() {
  const { t } = useTranslation();
  const asked = useStore((s) => s.confirmAsk);
  const answer = useStore((s) => s.answerConfirm);
  const cancelRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!asked) return;
    cancelRef.current?.focus();
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") answer(false); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [asked, answer]);
  useEffect(() => () => useStore.getState().answerConfirm(false), []);
  if (!asked) return null;
  return (
    <div className="fixed inset-0 z-[70] grid place-items-center glass-scrim anim-fade" onClick={() => answer(false)}>
      <div role="alertdialog" aria-modal="true" aria-labelledby="confirm-title" aria-describedby="confirm-message"
        className="w-[min(92vw,420px)] rounded-xl glass-panel anim-pop p-5" onClick={(e) => e.stopPropagation()}>
        <div id="confirm-title" className="flex items-center gap-2 font-semibold">
          {asked.danger && <TriangleAlert size={17} className="text-[var(--color-warn)] shrink-0" />}
          {t("confirm.title")}
        </div>
        <p id="confirm-message" className="mt-2 text-[13px] leading-relaxed text-[var(--color-muted)]">{asked.message}</p>
        <div className="mt-5 flex justify-end gap-2">
          <button ref={cancelRef} type="button" onClick={() => answer(false)}
            className="px-3 py-1.5 rounded-lg border border-[var(--color-border)] text-[13px] text-[var(--color-text)] hover:border-[var(--color-accent)] transition-colors">
            {t("common.cancel")}
          </button>
          <button type="button" onClick={() => answer(true)}
            className={`px-3 py-1.5 rounded-lg text-[13px] font-semibold transition ${asked.danger ? "bg-[#ef4444] text-white hover:brightness-110" : "bg-[var(--color-accent)] text-[var(--color-on-accent)] hover:brightness-105"}`}>
            {asked.confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
