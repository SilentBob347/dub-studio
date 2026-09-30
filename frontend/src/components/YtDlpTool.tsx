import { useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Loader2, RefreshCw } from "lucide-react";
import { ApiError, urlApi, type UrlTool } from "../lib/api";

// Раздел «Загрузка по ссылке» в «Моделях»: строка компонента и под ней версия yt-dlp — какая работает, последняя
// на GitHub, кнопка «Обновить». Новая версия встаёт рядом с закреплённой и работает только после проверки SHA-256
// и запуска.
export default function YtDlpTool({ installed, row }: { installed: boolean; row: ReactNode }) {
  const { t, i18n } = useTranslation();
  const [tool, setTool] = useState<UrlTool | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const fail = (e: unknown) => setErr(e instanceof ApiError ? `${e.code}: ${e.detail}` : e instanceof Error ? e.message : String(e));

  useEffect(() => {
    if (!installed) return;
    urlApi.tool().then(setTool, fail);
  }, [installed]);

  const updating = !!tool?.updating;
  useEffect(() => {
    if (!updating) return;
    const id = setInterval(() => { urlApi.tool().then(setTool, fail); }, 2000);
    return () => clearInterval(id);
  }, [updating]);

  const section = (body: ReactNode) => (
    <div className="mb-3">
      <div className="text-[11px] uppercase tracking-[0.14em] text-[var(--color-muted)] mb-1.5">{t("url.tool.group")}</div>
      <div className="space-y-1.5">{row}{body}</div>
    </div>
  );
  if (!installed || !tool) return section(err ? <div className="px-2.5 mono text-[10.5px] text-[var(--color-warn)] break-words">{err}</div> : null);

  const update = () => {
    setErr(null);
    urlApi.updateTool().then((r) => setTool(r.tool), fail);
  };
  const checked = tool.checkedAt > 0 ? new Date(tool.checkedAt * 1000).toLocaleString(i18n.language) : null;
  return section(
    <div className="px-2.5 py-2 rounded-lg bg-[var(--color-surface-2)] border border-[var(--color-border)]">
      <div className="flex items-center gap-2.5">
        <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${tool.updateAvailable ? "bg-[var(--color-warn)]" : "bg-[var(--color-accent)]"}`} />
        <div className="min-w-0 flex-1">
          <div className="text-[12px] font-medium truncate">
            {t("url.tool.version", { version: tool.version })} · {tool.updated ? t("url.tool.fromUpdate") : t("url.tool.pinned")}
          </div>
          <div className="mono text-[10px] text-[var(--color-muted)] truncate">
            {tool.latest ? t("url.tool.latest", { version: tool.latest }) : t("url.tool.never")}
            {checked ? ` · ${t("url.tool.checked", { when: checked })}` : ""}
          </div>
        </div>
        <button type="button" onClick={update} disabled={updating}
          className="shrink-0 inline-flex items-center gap-1 px-2.5 py-1 rounded-md text-[12px] border border-[var(--color-border)] text-[var(--color-accent-2)] hover:border-[var(--color-accent)] disabled:opacity-40">
          {updating ? <Loader2 size={12} className="animate-spin" /> : <RefreshCw size={12} />}{updating ? t("url.updating") : t("url.tool.update")}
        </button>
      </div>
      {tool.lastError && <div className="mt-1 mono text-[10.5px] text-[var(--color-warn)] break-words">{t("url.tool.failed", { error: tool.lastError })}</div>}
      {err && <div className="mt-1 mono text-[10.5px] text-[var(--color-warn)] break-words">{err}</div>}
    </div>
  );
}
