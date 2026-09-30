import { useEffect } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { AlertTriangle, Loader2 } from "lucide-react";

// Подтверждение необратимого действия вместо window.confirm (в нативном окне Tauri тот выглядит чужим и
// блокирует поток). Esc и клик по фону — отмена.
export default function ConfirmDialog({ open, title, message, confirmLabel, danger = true, busy = false, onConfirm, onCancel }: {
  open: boolean;
  title: string;
  message: string;
  confirmLabel: string;
  danger?: boolean;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape" && !busy) onCancel(); };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, busy, onCancel]);
  if (!open) return null;
  return createPortal(
    <div role="presentation" className="fixed inset-0 z-[200] grid place-items-center bg-black/60 backdrop-blur-sm px-4"
      onClick={(e) => { if (e.target === e.currentTarget && !busy) onCancel(); }}>
      <div role="alertdialog" aria-modal="true" aria-labelledby="confirm-title" aria-describedby="confirm-message"
        className="w-full max-w-sm rounded-xl border border-[var(--color-border)] bg-[var(--color-surface)] p-5 shadow-2xl">
        <div className="flex items-start gap-3">
          {danger && (
            <span className="shrink-0 grid place-items-center w-9 h-9 rounded-full bg-[color-mix(in_oklab,var(--color-danger,#ef4444)_14%,transparent)] text-[var(--color-danger,#ef4444)]">
              <AlertTriangle size={18} />
            </span>
          )}
          <div className="min-w-0 flex-1">
            <h3 id="confirm-title" className="text-[14px] font-semibold">{title}</h3>
            <p id="confirm-message" className="mt-1 text-[12.5px] leading-relaxed text-[var(--color-muted)] break-words">{message}</p>
          </div>
        </div>
        <div className="mt-5 flex justify-end gap-2">
          <button onClick={onCancel} disabled={busy}
            className="px-3 py-1.5 rounded-lg border border-[var(--color-border)] text-[12.5px] text-[var(--color-muted)] hover:text-[var(--color-text)] disabled:opacity-40">
            {t("common.cancel")}
          </button>
          <button onClick={onConfirm} disabled={busy} autoFocus
            className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-[12.5px] font-semibold disabled:opacity-60 ${danger ? "bg-[var(--color-danger,#ef4444)] text-white hover:brightness-110" : "bg-[var(--color-accent)] text-[var(--color-on-accent)] hover:brightness-105"}`}>
            {busy && <Loader2 size={13} className="animate-spin" />}{confirmLabel}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
