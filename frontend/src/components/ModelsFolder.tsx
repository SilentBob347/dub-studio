import { useState } from "react";
import { useTranslation } from "react-i18next";
import { FolderOpen } from "lucide-react";
import { api, SetupError, type SetupStatus } from "../lib/api";
import { fmtBytes } from "../lib/format";
import { useDownloadErrorText } from "../lib/setupText";

// Где лежат модели, сколько там свободно и сколько нужно под выбранное; открыть папку в проводнике.
export default function ModelsFolder({ status, need = 0 }: { status: SetupStatus; need?: number }) {
  const { t } = useTranslation();
  const errText = useDownloadErrorText();
  const [err, setErr] = useState<string | null>(null);
  const free = status.freeBytes;
  const short = free != null && need > free;
  const open = () => {
    setErr(null);
    api.setupOpenModels().catch((e) => setErr(e instanceof SetupError ? `${errText(e.code)} · ${e.detail}` : String(e)));
  };
  return (
    <div className="rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-2)] px-3 py-2">
      <div className="flex items-center gap-2.5">
        <div className="min-w-0 flex-1">
          <div className="text-[11px] uppercase tracking-[0.14em] text-[var(--color-muted)]">{t("downloads.folder")}</div>
          <div className="mono text-[11px] truncate" title={status.modelsDir}>{status.modelsDir}</div>
          <div className={`mono text-[10.5px] ${short ? "text-[var(--color-warn)]" : "text-[var(--color-muted)]"}`}>
            {free == null ? t("downloads.freeUnknown") : t("downloads.free", { size: fmtBytes(free) })}
            {need > 0 && ` · ${t("downloads.need", { size: fmtBytes(need) })}`}
          </div>
        </div>
        <button onClick={open}
          className="shrink-0 inline-flex items-center gap-1 px-2.5 py-1 rounded-md border border-[var(--color-border)] text-[12px] text-[var(--color-muted)] hover:border-[var(--color-accent)] hover:text-[var(--color-text)]">
          <FolderOpen size={12} />{t("downloads.openFolder")}
        </button>
      </div>
      {short && <div className="mt-1 text-[11.5px] text-[var(--color-warn)]">{t("downloads.noSpace", { need: fmtBytes(need), free: fmtBytes(free ?? 0) })}</div>}
      {err && <div className="mt-1 mono text-[10.5px] text-[var(--color-warn)] break-words">{err}</div>}
    </div>
  );
}
