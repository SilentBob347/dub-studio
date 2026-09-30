import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { AudioLines, Cookie, Download, Film, Link2, Loader2, Play, RefreshCw, Settings, X } from "lucide-react";
import { ApiError, urlApi, type UrlFetch, type UrlProbe, type UrlQuality, type UrlTool } from "../lib/api";
import { fmtBytes } from "../lib/format";
import { openSettings } from "../lib/settingsNav";
import { useStore } from "../store";

// Коды, при которых помогает cookies.txt вошедшего браузера, настройки сети или новый yt-dlp.
const COOKIE_CODES = new Set(["login_required", "age_restricted", "private", "members_only", "cookies_invalid"]);
const NETWORK_CODES = new Set(["geo_blocked", "proxy", "network", "rate_limited"]);
const MODELS_CODES = new Set(["tool_missing", "ffmpeg_missing"]);
const COOKIES_LIMIT = 1024 * 1024;

type Failure = { code: string; detail: string };

const failureOf = (e: unknown): Failure =>
  e instanceof ApiError ? { code: e.code, detail: e.detail } : { code: "generic", detail: e instanceof Error ? e.message : String(e) };

function clock(sec: number): string {
  const s = Math.max(0, Math.round(sec));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${r}` : `${m}:${r}`;
}

// Тексты по кодам сервера: ошибка, предупреждение о субтитрах, фаза загрузки.
function useUrlText() {
  const { t } = useTranslation();
  const errors: Record<string, string> = {
    tool_missing: t("url.err.tool_missing"),
    ffmpeg_missing: t("url.err.ffmpeg_missing"),
    bad_url: t("url.err.bad_url"),
    bad_quality: t("url.err.bad_quality"),
    unsupported_url: t("url.err.unsupported_url"),
    playlist: t("url.err.playlist"),
    live: t("url.err.live"),
    geo_blocked: t("url.err.geo_blocked"),
    age_restricted: t("url.err.age_restricted"),
    login_required: t("url.err.login_required"),
    private: t("url.err.private"),
    members_only: t("url.err.members_only"),
    drm: t("url.err.drm"),
    unavailable: t("url.err.unavailable"),
    rate_limited: t("url.err.rate_limited"),
    proxy: t("url.err.proxy"),
    network: t("url.err.network"),
    cookies_invalid: t("url.err.cookies_invalid"),
    format_unavailable: t("url.err.format_unavailable"),
    no_audio: t("url.err.no_audio"),
    outdated: t("url.err.outdated"),
    disk_space: t("url.err.disk_space"),
    busy: t("url.err.busy"),
    interrupted: t("url.err.interrupted"),
    update_failed: t("url.err.update_failed"),
    io: t("url.err.io"),
  };
  const warnings: Record<string, string> = {
    subs_missing: t("url.warn.subs_missing"),
    subs_failed: t("url.warn.subs_failed"),
    subs_empty: t("url.warn.subs_empty"),
  };
  const phases: Record<string, string> = {
    probe: t("url.phase.probe"),
    download: t("url.phase.download"),
    merge: t("url.phase.merge"),
    extract: t("url.phase.extract"),
    subtitles: t("url.phase.subtitles"),
    project: t("url.phase.project"),
    done: t("url.phase.done"),
  };
  return {
    error: (code: string) => errors[code] ?? t("url.err.generic"),
    warning: (code: string) => warnings[code] ?? t("url.warn.generic"),
    phase: (p: string) => phases[p] ?? t("url.phase.download"),
  };
}

// Поле «Вставить ссылку» стартового экрана: проба ссылки -> карточка (превью, качество, субтитры площадки,
// cookies.txt) -> загрузка в фоне мимо очереди джоб -> проект открывается сам.
export default function UrlImport({ onOpen }: { onOpen: (pid: string) => void }) {
  const { t } = useTranslation();
  const text = useUrlText();
  const [tool, setTool] = useState<UrlTool | null>(null);
  const [url, setUrl] = useState("");
  const [cookies, setCookies] = useState<{ name: string; text: string } | null>(null);
  const [probing, setProbing] = useState(false);
  const [probe, setProbe] = useState<UrlProbe | null>(null);
  const [quality, setQuality] = useState<UrlQuality>("best");
  const [subsLang, setSubsLang] = useState("");
  const [failure, setFailure] = useState<Failure | null>(null);
  const [job, setJob] = useState<UrlFetch | null>(null);
  const [updating, setUpdating] = useState(false);
  const [updatedTo, setUpdatedTo] = useState<string | null>(null);
  const shownProgress = useRef(false);
  const lastStatus = useRef<string | null>(null);

  const log = (msg: string, kind: "work" | "done" | "error") => useStore.getState().pushActivity(msg, kind);

  useEffect(() => {
    urlApi.tool().then(setTool, (e: unknown) => setFailure(failureOf(e)));
    // Загрузка, идущая или прерванная закрытием студии, видна и после перезапуска окна.
    urlApi.list().then(
      (r) => {
        const open = r.fetches.find((f) => f.status === "downloading" || f.status === "interrupted");
        if (open) { lastStatus.current = open.status; setJob(open); }
      },
      (e: unknown) => setFailure(failureOf(e)),
    );
  }, []);

  // Компонент ставят в «Моделях», пока стартовый экран открыт: поле появляется, как только он скачан.
  const missing = tool !== null && !tool.installed;
  useEffect(() => {
    if (!missing) return;
    const id = setInterval(() => { urlApi.tool().then(setTool, (e: unknown) => setFailure(failureOf(e))); }, 5000);
    return () => clearInterval(id);
  }, [missing]);

  const downloading = job?.status === "downloading";
  useEffect(() => {
    if (!downloading || !job) return;
    const id = setInterval(() => {
      urlApi.get(job.id).then(setJob, (e: unknown) => setFailure(failureOf(e)));
    }, 1000);
    return () => clearInterval(id);
  }, [downloading, job?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  // Шапка окна показывает загрузку, как закачку моделей; итог — в журнале.
  useEffect(() => {
    if (!job) return;
    const store = useStore.getState();
    if (job.status === "downloading") {
      const pct = job.total ? Math.min(100, Math.round((job.downloaded / job.total) * 100)) : 0;
      store.setProgress("", t("url.header", { title: job.title ?? job.url }), pct);
      shownProgress.current = true;
    } else if (shownProgress.current) {
      store.setProgress("", "", null);
      shownProgress.current = false;
    }
    if (lastStatus.current === job.status) return;
    const was = lastStatus.current;
    lastStatus.current = job.status;
    if (was !== "downloading") return;
    if (job.status === "completed" && job.pid) {
      log(t("url.logDone", { title: job.title ?? job.url }), "done");
      if (job.warning) log(`${text.warning(job.warning)}${job.warningDetail ? ` · ${job.warningDetail}` : ""}`, "error");
      onOpen(job.pid);
    } else if (job.status === "failed") {
      log(t("url.logFailed", { error: `${text.error(job.errorCode ?? "generic")}${job.error ? ` · ${job.error}` : ""}` }), "error");
    }
  }, [job]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => () => {
    if (shownProgress.current) useStore.getState().setProgress("", "", null);
  }, []);

  useEffect(() => {
    if (!updating) return;
    const id = setInterval(() => {
      urlApi.tool().then((s) => {
        setTool(s);
        if (s.updating) return;
        setUpdating(false);
        if (s.lastError) setFailure({ code: "update_failed", detail: s.lastError });
        else setUpdatedTo(s.version);
      }, (e: unknown) => { setUpdating(false); setFailure(failureOf(e)); });
    }, 2000);
    return () => clearInterval(id);
  }, [updating]);

  const check = async (typed?: string) => {
    const link = (typed ?? url).trim();
    if (!link || probing) return;
    setProbing(true); setFailure(null); setProbe(null); setUpdatedTo(null);
    try {
      const p = await urlApi.probe(link, cookies?.text ?? null);
      setProbe(p);
      setQuality(p.qualities.includes("1080") ? "1080" : p.qualities[0] ?? "best");
      setSubsLang("");
    } catch (e) {
      setFailure(failureOf(e));
    } finally {
      setProbing(false);
    }
  };

  const start = async () => {
    if (!probe) return;
    setFailure(null);
    try {
      const r = await urlApi.start({ url: probe.url || url.trim(), quality, subs_lang: subsLang || null, cookies_text: cookies?.text ?? null });
      lastStatus.current = r.fetch.status;
      setJob(r.fetch);
      setProbe(null);
      log(t("url.logStarted", { title: probe.title }), "work");
    } catch (e) {
      setFailure(failureOf(e));
    }
  };

  const act = (run: () => Promise<{ fetch: UrlFetch }>) => {
    setFailure(null);
    run().then((r) => { lastStatus.current = r.fetch.status === "downloading" ? "downloading" : lastStatus.current; setJob(r.fetch); }, (e: unknown) => setFailure(failureOf(e)));
  };
  const forget = () => {
    if (!job) return;
    urlApi.forget(job.id).then(() => setJob(null), (e: unknown) => setFailure(failureOf(e)));
  };
  // После ошибки, которую лечит cookies.txt: убрать упавшую загрузку и проверить ссылку заново с выбранным файлом.
  const recheck = () => {
    if (!job) { void check(); return; }
    const link = job.url;
    urlApi.forget(job.id).then(() => { setJob(null); setUrl(link); void check(link); }, (e: unknown) => setFailure(failureOf(e)));
  };
  const updateTool = () => {
    setFailure(null); setUpdatedTo(null);
    urlApi.updateTool().then((r) => { setTool(r.tool); setUpdating(true); }, (e: unknown) => setFailure(failureOf(e)));
  };

  const pickCookies = (f: File | undefined) => {
    if (!f) return;
    if (f.size > COOKIES_LIMIT) { setFailure({ code: "cookies_invalid", detail: f.name }); return; }
    f.text().then((body) => setCookies({ name: f.name, text: body }), (e: unknown) => setFailure(failureOf(e)));
  };

  const qualityLabel = (q: UrlQuality) => (q === "best" ? t("url.qBest") : q === "audio" ? t("url.qAudio") : t("url.qHeight", { h: q }));

  const actions = (code: string) => {
    const btn = "inline-flex items-center gap-1 px-2 py-1 rounded-md border border-[var(--color-border)] text-[11.5px] text-[var(--color-text)] hover:border-[var(--color-accent)]";
    return (
      <div className="mt-1.5 flex flex-wrap gap-1.5">
        {COOKIE_CODES.has(code) && (
          <button type="button" onClick={recheck} disabled={!cookies} title={cookies ? "" : t("url.cookiesTip")} className={`${btn} disabled:opacity-40`}><RefreshCw size={12} />{t("url.checkAgain")}</button>
        )}
        {NETWORK_CODES.has(code) && <button type="button" onClick={() => openSettings("network")} className={btn}><Settings size={12} />{t("url.openNetwork")}</button>}
        {MODELS_CODES.has(code) && <button type="button" onClick={() => openSettings("models")} className={btn}><Settings size={12} />{t("url.openModels")}</button>}
        {code === "outdated" && (
          <button type="button" onClick={updateTool} disabled={updating} className={`${btn} disabled:opacity-40`}>
            {updating ? <Loader2 size={12} className="animate-spin" /> : <RefreshCw size={12} />}{updating ? t("url.updating") : t("url.updateTool")}
          </button>
        )}
      </div>
    );
  };

  const failureBox = (f: Failure) => (
    <div className="mt-2 rounded-lg border border-[var(--color-warn)]/40 bg-[color-mix(in_oklab,var(--color-warn)_10%,transparent)] px-2.5 py-2" role="alert">
      <div className="text-[12px] text-[var(--color-warn)] leading-snug">{text.error(f.code)}</div>
      {f.detail && <div className="mt-0.5 mono text-[10.5px] text-[var(--color-muted)] break-words">{f.detail}</div>}
      {actions(f.code)}
    </div>
  );

  if (tool && !tool.installed) {
    return (
      <div className="mt-3 flex items-center gap-2 rounded-xl border border-dashed border-[var(--color-border)] px-3 py-2 text-[12px] text-[var(--color-muted)]">
        <Link2 size={15} className="shrink-0 text-[var(--color-accent-2)]" />
        <span className="flex-1 leading-snug">{t("url.toolMissing")}</span>
        <button type="button" onClick={() => openSettings("models")}
          className="shrink-0 inline-flex items-center gap-1 px-2 py-1 rounded-md border border-[var(--color-border)] text-[11.5px] text-[var(--color-text)] hover:border-[var(--color-accent)]">
          <Settings size={12} />{t("url.openModels")}
        </button>
      </div>
    );
  }

  const running = job?.status === "downloading";
  const pct = job && job.total ? Math.min(100, (job.downloaded / job.total) * 100) : null;
  const jobTitle = !job ? "" : running ? text.phase(job.phase)
    : job.status === "interrupted" ? t("url.interrupted")
    : job.status === "failed" ? text.error(job.errorCode ?? "generic")
    : job.status === "cancelled" ? t("url.cancelled")
    : text.phase("done");

  return (
    <div className="mt-3 rounded-xl border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-2.5">
      <form onSubmit={(e) => { e.preventDefault(); void check(); }} className="flex items-center gap-2">
        <Link2 size={15} className="shrink-0 text-[var(--color-accent-2)]" />
        <input value={url} onChange={(e) => setUrl(e.target.value)} placeholder={t("url.placeholder")} aria-label={t("url.placeholder")}
          disabled={running} spellCheck={false}
          className="flex-1 min-w-0 bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-lg px-2.5 py-1.5 text-[13px] focus:border-[var(--color-accent)] focus:outline-none disabled:opacity-50" />
        <button type="submit" disabled={!url.trim() || probing || running}
          className="shrink-0 inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-[var(--color-border)] text-[12.5px] font-medium text-[var(--color-text)] hover:border-[var(--color-accent)] disabled:opacity-40">
          {probing && <Loader2 size={13} className="animate-spin" />}{probing ? t("url.checking") : t("url.check")}
        </button>
      </form>
      <div className="mt-1.5 flex items-center gap-1.5 text-[11px]">
        <label title={t("url.cookiesTip")}
          className={`inline-flex items-center gap-1 cursor-pointer transition-colors ${cookies ? "text-[var(--color-accent-2)]" : "text-[var(--color-muted)] hover:text-[var(--color-accent-2)]"}`}>
          <Cookie size={12} />
          <span className="truncate max-w-[220px]">{cookies ? cookies.name : t("url.cookiesPick")}</span>
          <input type="file" accept=".txt,text/plain" className="hidden" onChange={(e) => { pickCookies(e.target.files?.[0]); e.currentTarget.value = ""; }} />
        </label>
        {cookies && (
          <button type="button" onClick={() => setCookies(null)} title={t("url.cookiesClear")} aria-label={t("url.cookiesClear")}
            className="p-0.5 rounded text-[var(--color-muted)] hover:text-[var(--color-text)]"><X size={11} /></button>
        )}
        {updatedTo && <span className="ml-auto text-[var(--color-accent-2)]">{t("url.updated", { version: updatedTo })}</span>}
      </div>

      {failure && failureBox(failure)}

      {probe && !running && (
        <div className="mt-3">
          <div className="flex gap-3">
            <div className="w-28 h-16 shrink-0 rounded-md overflow-hidden bg-black/40 grid place-items-center text-[var(--color-muted)]">
              {probe.thumbnail_data
                ? <img src={probe.thumbnail_data} alt="" className="w-full h-full object-cover" />
                : probe.has_video ? <Film size={18} /> : <AudioLines size={18} />}
            </div>
            <div className="min-w-0 flex-1">
              <div className="text-[13px] font-medium leading-snug line-clamp-2 break-words" title={probe.title}>{probe.title}</div>
              <div className="mt-0.5 mono text-[11px] text-[var(--color-muted)] truncate">
                {[probe.uploader, probe.duration != null ? clock(probe.duration) : null, probe.extractor].filter(Boolean).join(" · ")}
              </div>
            </div>
          </div>
          <div className="mt-2.5 grid grid-cols-2 gap-2">
            <label className="flex flex-col gap-1 text-[11px] text-[var(--color-muted)]">
              {t("url.quality")}
              <select value={quality} onChange={(e) => setQuality(e.target.value as UrlQuality)}
                className="bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-md px-2 py-1 text-[12px] text-[var(--color-text)] focus:border-[var(--color-accent)] focus:outline-none">
                {probe.qualities.map((q) => <option key={q} value={q}>{qualityLabel(q)}</option>)}
              </select>
            </label>
            <label className="flex flex-col gap-1 text-[11px] text-[var(--color-muted)]" title={t("url.subsHint")}>
              {t("url.subs")}
              <select value={subsLang} onChange={(e) => setSubsLang(e.target.value)} disabled={probe.subtitles.length === 0}
                className="bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-md px-2 py-1 text-[12px] text-[var(--color-text)] focus:border-[var(--color-accent)] focus:outline-none disabled:opacity-50">
                <option value="">{probe.subtitles.length ? t("url.subsNone") : t("url.noSubs")}</option>
                {probe.subtitles.map((tr) => <option key={tr.lang} value={tr.lang}>{tr.name ? `${tr.name} (${tr.lang})` : tr.lang}</option>)}
              </select>
            </label>
          </div>
          {subsLang && <p className="mt-1.5 text-[10.5px] leading-snug text-[var(--color-accent-2)]">{t("url.subsHint")}</p>}
          {probe.auto_subtitles.length > 0 && <p className="mt-1 text-[10.5px] leading-snug text-[var(--color-muted)]">{t("url.subsAuto")}</p>}
          <button type="button" onClick={() => { void start(); }}
            className="mt-2.5 w-full inline-flex items-center justify-center gap-2 px-4 py-2 rounded-lg bg-[var(--color-accent)] text-[var(--color-on-accent)] text-[13px] font-semibold hover:brightness-105">
            <Download size={15} />{t("url.download")}
            {quality === "best" && probe.expected_bytes ? <span className="font-normal opacity-80">{`≈ ${fmtBytes(probe.expected_bytes)}`}</span> : null}
          </button>
        </div>
      )}

      {job && (
        <div className="mt-3 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-2)] px-3 py-2.5" aria-live="polite">
          <div className="flex items-center gap-2">
            {running && <Loader2 size={14} className="animate-spin text-[var(--color-accent)] shrink-0" />}
            <span className={`text-[12.5px] font-medium flex-1 min-w-0 truncate ${job.status === "failed" ? "text-[var(--color-warn)]" : ""}`}>{jobTitle}</span>
            {running ? (
              <button type="button" onClick={() => act(() => urlApi.cancel(job.id))}
                className="shrink-0 px-2.5 py-1 rounded-md border border-[var(--color-border)] text-[12px] text-[var(--color-muted)] hover:text-[var(--color-text)]">{t("url.cancel")}</button>
            ) : (job.status === "interrupted" || job.status === "failed") && (
              <button type="button" onClick={() => act(() => urlApi.resume(job.id))}
                className="shrink-0 inline-flex items-center gap-1 px-2.5 py-1 rounded-md bg-[var(--color-accent)] text-[var(--color-on-accent)] text-[12px] font-semibold hover:brightness-105">
                <Play size={12} />{job.status === "failed" ? t("url.retry") : t("url.resume")}
              </button>
            )}
            {!running && (
              <button type="button" onClick={forget} title={t("url.dismiss")} aria-label={t("url.dismiss")}
                className="shrink-0 p-1 rounded-md text-[var(--color-muted)] hover:text-[var(--color-text)]"><X size={13} /></button>
            )}
          </div>
          <div className="mt-0.5 mono text-[11px] text-[var(--color-muted)] truncate" title={job.url}>{job.title ?? job.url}</div>
          {running && pct != null && (
            <div className="mt-2 h-1.5 rounded-full bg-[var(--color-surface)] overflow-hidden" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(pct)}>
              <div className="h-full bg-[var(--color-accent)] transition-[width] duration-300" style={{ width: `${pct}%` }} />
            </div>
          )}
          {running && job.downloaded > 0 && (
            <div className="mt-1.5 flex flex-wrap gap-x-3 mono text-[11px] text-[var(--color-muted)]">
              <span>{job.total ? t("downloads.progress", { done: fmtBytes(job.downloaded), total: fmtBytes(job.total) }) : fmtBytes(job.downloaded)}</span>
              {job.speedBps > 0 && <span>{t("downloads.speed", { speed: fmtBytes(job.speedBps) })}</span>}
              {job.etaS != null && job.etaS > 0 && <span>{t("url.eta", { time: clock(job.etaS) })}</span>}
            </div>
          )}
          {job.status === "failed" && job.error && <div className="mt-1 mono text-[10.5px] text-[var(--color-muted)] break-words">{job.error}</div>}
          {job.status === "failed" && job.errorCode && actions(job.errorCode)}
        </div>
      )}

      {(probe || job) && <p className="mt-2 text-[10.5px] leading-snug text-[var(--color-muted)]">{t("url.rights")}</p>}
    </div>
  );
}
