import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Eye, EyeOff } from "lucide-react";
import { api, ApiError, type ProxySettings } from "../lib/api";

// Прокси для всего исходящего трафика (закачка моделей + OpenRouter). Сервер отдаёт адрес без пароля и флаг
// «пароль задан»: пустое поле пароля при сохранении значит «оставить сохранённый».
export default function ProxySection() {
  const { t } = useTranslation();
  const [loaded, setLoaded] = useState(false);
  const [url, setUrl] = useState("");
  const [password, setPassword] = useState("");
  const [passwordSet, setPasswordSet] = useState(false);
  const [on, setOn] = useState(false);
  const [show, setShow] = useState(false);
  const [testing, setTesting] = useState(false);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const apply = (s: ProxySettings) => { setUrl(s.url); setOn(s.on); setPasswordSet(s.password_set); setPassword(""); setLoaded(true); };
  const detail = (e: unknown) => (e instanceof ApiError ? e.detail || e.code : e instanceof Error ? e.message : String(e));
  useEffect(() => {
    api.proxySettings().then(apply).catch((e: unknown) => setMsg({ ok: false, text: t("secrets.loadFailed", { detail: detail(e) }) }));
  }, [t]);
  const save = async (nextOn: boolean) => {
    setMsg(null);
    const form: { on: boolean; url: string; password?: string } = { on: nextOn, url: url.trim() };
    if (password.trim()) form.password = password.trim();
    try { apply(await api.saveProxy(form)); } catch (e) {
      setMsg({ ok: false, text: e instanceof ApiError && e.code === "proxy_password_without_user" ? t("secrets.proxyPasswordNeedsUser") : t("secrets.proxySaveFailed", { detail: detail(e) }) });
    }
  };
  const test = async () => {
    setTesting(true); setMsg(null);
    try {
      const r = await api.proxyTest(url.trim(), password.trim() || undefined);
      if (r.ok) setMsg({ ok: true, text: t("secrets.proxyOk") });
      else if (r.error) setMsg({ ok: false, text: r.error });
      else {
        const bad = [r.hf === false ? t("secrets.proxyHf") : "", r.openrouter === false ? t("secrets.proxyOr") : ""].filter(Boolean);
        setMsg({ ok: false, text: bad.length ? t("secrets.proxyUnreachable", { what: bad.join(", ") }) : t("secrets.proxyTestFailed") });
      }
    } catch { setMsg({ ok: false, text: t("secrets.proxyTestFailed") }); }
    setTesting(false);
  };
  const fieldCls = "flex-1 min-w-0 px-2 py-1 rounded-md bg-[var(--color-surface)] border border-[var(--color-border)] text-[12px] mono focus:border-[var(--color-accent)] outline-none disabled:opacity-60";
  return (
    <div className="px-2.5 py-2 rounded-lg bg-[var(--color-surface-2)] border border-[var(--color-border)] space-y-2">
      <label className="flex items-center gap-2.5 cursor-pointer select-none">
        <input type="checkbox" checked={on} disabled={!loaded} onChange={(e) => save(e.target.checked)}
          className="accent-[var(--color-accent)] w-3.5 h-3.5 shrink-0" />
        <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${on ? "bg-[var(--color-accent)]" : "bg-[var(--color-muted)]"}`} />
        <span className="min-w-0 flex-1">
          <span className="block text-[12px] font-medium">{t("secrets.proxyToggle")}</span>
          <span className="block mono text-[10px] text-[var(--color-muted)]">{t("secrets.proxyToggleHint")}</span>
        </span>
      </label>
      <div className="flex gap-2">
        <input type="text" value={url} disabled={!loaded} onChange={(e) => setUrl(e.target.value)} onBlur={() => { if (on && url.trim()) save(true); }}
          placeholder={t("secrets.proxyUrlPlaceholder")} aria-label={t("secrets.proxyUrl")} autoComplete="off" spellCheck={false} className={fieldCls} />
        <button onClick={test} disabled={testing || !url.trim()} className="px-3 py-1 rounded-md bg-[var(--color-surface)] border border-[var(--color-border)] text-[12px] hover:border-[var(--color-accent)] disabled:opacity-40">{testing ? "…" : t("secrets.proxyTest")}</button>
      </div>
      <div className="flex gap-2">
        <input type={show ? "text" : "password"} data-mcp-secret value={password} disabled={!loaded} onChange={(e) => setPassword(e.target.value)}
          onBlur={() => { if (on && url.trim() && password.trim()) save(true); }}
          placeholder={passwordSet ? t("secrets.proxyPasswordSaved") : t("secrets.proxyPasswordPlaceholder")} aria-label={t("secrets.proxyPassword")}
          autoComplete="new-password" className={fieldCls} />
        <button onClick={() => setShow((s) => !s)} aria-label={show ? t("secrets.hide") : t("secrets.show")} title={show ? t("secrets.hide") : t("secrets.show")}
          className="px-2 rounded-md border border-[var(--color-border)] text-[var(--color-muted)] hover:text-[var(--color-text)]">{show ? <EyeOff size={13} /> : <Eye size={13} />}</button>
      </div>
      {msg && <div className={`text-[11px] ${msg.ok ? "text-[var(--color-accent)]" : "text-[var(--color-warn)]"}`}>{msg.text}</div>}
      <div className="mono text-[10px] text-[var(--color-muted)] leading-snug">{t("secrets.proxyNote")}</div>
    </div>
  );
}
