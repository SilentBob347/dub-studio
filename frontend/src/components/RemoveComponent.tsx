import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Trash2 } from "lucide-react";
import { api, SetupError, type SetupComponent, type SetupStatus } from "../lib/api";
import { fmtBytes } from "../lib/format";
import ConfirmDialog from "./ConfirmDialog";
import { useDownloadErrorText } from "../lib/setupText";

// Удалить скачанный компонент (и недокачанный остаток) с подтверждением, сколько места освободится.
export default function RemoveComponent({ c, disabled, onDone, onError }: {
  c: SetupComponent;
  disabled?: boolean;
  onDone: (status: SetupStatus, freed: number) => void;
  onError: (msg: string) => void;
}) {
  const { t } = useTranslation();
  const errText = useDownloadErrorText();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  if (c.delivery !== "download" || c.bytesOnDisk <= 0) return null;
  const confirm = async () => {
    setBusy(true);
    try {
      const r = await api.setupRemove([c.id]);
      onDone(r.status, r.freedBytes);
      if (r.errors.length > 0) onError(`${t("downloads.removeErrors")} · ${r.errors.join("; ")}`);
    } catch (e) {
      onError(e instanceof SetupError ? `${errText(e.code)} · ${e.detail}` : String(e));
    } finally {
      setBusy(false);
      setOpen(false);
    }
  };
  return (
    <>
      <button onClick={() => setOpen(true)} disabled={disabled} title={t("downloads.remove")} aria-label={t("downloads.remove")}
        className="shrink-0 inline-flex items-center px-1.5 py-1 rounded-md border border-[var(--color-border)] text-[var(--color-muted)] hover:border-[var(--color-danger,#ef4444)] hover:text-[var(--color-danger,#ef4444)] disabled:opacity-40">
        <Trash2 size={12} />
      </button>
      <ConfirmDialog open={open} busy={busy}
        title={t("downloads.removeTitle", { name: c.name })}
        message={t("downloads.removeConfirm", { size: fmtBytes(c.bytesOnDisk) })}
        confirmLabel={t("downloads.remove")}
        onConfirm={confirm} onCancel={() => setOpen(false)} />
    </>
  );
}
