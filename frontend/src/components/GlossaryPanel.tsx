import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { BookA, BookUp, Check, Download, Loader2, Plus, RotateCw, Search, Sparkles, Trash2, TriangleAlert, Upload, X } from "lucide-react";
import { api, JobCancelledError, type GlossaryEntry, type Project } from "../lib/api";
import { watchLocal } from "../lib/jobs";
import { useStore } from "../store";
import ConfirmDialog from "./ConfirmDialog";

type Row = { key: number; entry: GlossaryEntry; asr: string };

let nextKey = 1;
const toRow = (entry: GlossaryEntry): Row => ({ key: nextKey++, entry, asr: (entry.asr_fix || []).join(", ") });
const fromRow = (r: Row): GlossaryEntry => ({ ...r.entry, asr_fix: r.asr.split(",").map((s) => s.trim()).filter(Boolean) });
const norm = (s: string) => s.trim().toLowerCase().normalize("NFD").replace(/\u0308/g, "").normalize("NFC").replace(/\s+/g, " ");
const blank = (lang: string): GlossaryEntry => ({ term: "", translation: "", keep: false, pronunciation: "", asr_fix: [], note: "", source: "manual", lang });
const errText = (e: unknown) => (e instanceof Error ? e.message : String(e));

// Режим, в котором проект переводится заново (retranslate): только режимы с переводом.
const retranslateMode = (p: Project): string | null => {
  if (p.audio.rewrite) return null;
  if (p.mode === "dub" || p.mode === "voiceover") return p.mode;
  if (p.mode === "nodub" && p.subs.mode === "translate") return "nodub";
  return null;
};

const CELL = "w-full min-w-0 bg-[var(--color-bg)]/60 border border-[var(--color-border)] rounded-md px-1.5 py-1 text-[12px] focus:border-[var(--color-accent)] focus:outline-none transition-colors";
const BTN = "inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-2)] text-[12px] hover:border-[var(--color-accent)] hover:text-[var(--color-accent)] disabled:opacity-40 disabled:hover:border-[var(--color-border)] disabled:hover:text-inherit transition-colors";

// Глоссарий проекта: термины для перевода, распознавания и озвучки; «Собрать из текста», TSV, профиль сериала.
export default function GlossaryPanel({ pid, project, onClose }: { pid: string; project: Project; onClose: () => void }) {
  const { t } = useTranslation();
  const setProject = useStore((s) => s.setProject);
  const bump = useStore((s) => s.bump);
  const pushActivity = useStore((s) => s.pushActivity);
  const [rows, setRows] = useState<Row[] | null>(null);
  const [saved, setSaved] = useState<string>("[]");
  const [stale, setStale] = useState(false);
  const [castingRef, setCastingRef] = useState("");
  const [filter, setFilter] = useState("");
  const [busy, setBusy] = useState<null | "save" | "extract" | "import" | "export" | "series" | "retranslate">(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [candidates, setCandidates] = useState<GlossaryEntry[]>([]);
  const [profiles, setProfiles] = useState<{ slug: string; name: string }[]>([]);
  const [series, setSeries] = useState("");
  const [confirmClose, setConfirmClose] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);
  const lang = project.tgt_lang;

  const load = (g: { entries: GlossaryEntry[]; stale: boolean; casting_ref: string }) => {
    setRows(g.entries.map(toRow));
    setSaved(JSON.stringify(g.entries));
    setStale(g.stale);
    setCastingRef(g.casting_ref);
  };
  useEffect(() => {
    api.glossary(pid).then(load, (e) => setError(errText(e)));
    api.castingLibrary().then((r) => setProfiles(r.casts.map((c) => ({ slug: c.slug, name: c.name }))), (e) => setError(errText(e)));
  }, [pid]);
  // Профиль по умолчанию — тот, что применён к проекту при анализе.
  const seriesSlug = series || (profiles.some((p) => p.slug === castingRef) ? castingRef : profiles[0]?.slug ?? "");

  const entries = useMemo(() => (rows || []).map(fromRow), [rows]);
  const dirty = rows !== null && JSON.stringify(entries) !== saved;
  const shown = (rows || []).filter((r) => {
    const f = norm(filter);
    return !f || !r.entry.term || [r.entry.term, r.entry.translation, r.entry.pronunciation, r.entry.note, r.asr].some((v) => norm(v).includes(f));
  });

  const edit = (key: number, patch: Partial<GlossaryEntry>, asr?: string) =>
    setRows((rs) => (rs || []).map((r) => (r.key === key ? { ...r, entry: { ...r.entry, ...patch, source: "manual" }, asr: asr ?? r.asr } : r)));

  // Правки окна сохраняются до любого действия над сохранённым глоссарием (импорт, экспорт, профиль, перевод).
  async function save(): Promise<GlossaryEntry[]> {
    const list = entries.filter((e) => e.term.trim());
    const g = await api.saveGlossary(pid, { entries: list });
    load(g);
    return g.entries;
  }
  async function run(kind: NonNullable<typeof busy>, action: () => Promise<void>) {
    setBusy(kind);
    setError(null);
    setNotice(null);
    try { await action(); } catch (e) { setError(errText(e)); } finally { setBusy(null); }
  }

  const doSave = () => run("save", async () => { await save(); setNotice(t("glossary.saved")); });
  const doExtract = () => run("extract", async () => {
    if (dirty) await save();
    const { job_id } = await api.extractGlossary(pid);
    try {
      const res = (await watchLocal(pid, "glossary", job_id, () => {})) as { entries: GlossaryEntry[] };
      const have = new Set(entries.map((e) => norm(e.term)));
      const found = res.entries.filter((e) => !have.has(norm(e.term)));
      setCandidates(found);
      setNotice(found.length ? null : t("glossary.nothingFound"));
    } catch (e) {
      if (e instanceof JobCancelledError) { setNotice(t("jobs.cancelledKind", { kind: t("jobs.kind.glossary") })); return; }
      throw e;
    }
  });
  const doImport = (file: File) => run("import", async () => {
    const tsv = await file.text();
    if (dirty) await save();
    load(await api.saveGlossary(pid, { tsv, merge: true, lang }));
    setNotice(t("glossary.imported"));
  });
  const doExport = () => run("export", async () => {
    if (dirty) await save();
    const r = await api.saveText(pid, "glossary.tsv", await api.glossaryTsv(pid));
    setNotice(t("glossary.exported", { path: r.path }));
  });
  const doSeries = () => run("series", async () => {
    const list = dirty ? await save() : entries;
    await api.saveSeriesGlossary(seriesSlug, { entries: list, merge: true });
    setNotice(t("glossary.seriesSaved", { name: profiles.find((p) => p.slug === seriesSlug)?.name ?? seriesSlug }));
  });
  const mode = retranslateMode(project);
  const doRetranslate = () => run("retranslate", async () => {
    if (!mode) return;
    if (dirty) await save();
    const { job_id } = await api.retranslate(pid, lang, mode);
    try {
      await watchLocal(pid, "retranslate", job_id, () => {});
    } catch (e) {
      if (e instanceof JobCancelledError) { setNotice(t("jobs.cancelledKind", { kind: t("jobs.kind.retranslate") })); return; }
      throw e;
    }
    setProject(await api.getProject(pid));
    bump();
    load(await api.glossary(pid));
    pushActivity(t("glossary.retranslated"), "done");
  });

  const accept = (list: GlossaryEntry[]) => {
    setRows((rs) => [...(rs || []), ...list.map((e) => toRow({ ...blank(lang), ...e }))]);
    setCandidates((cs) => cs.filter((c) => !list.includes(c)));
  };
  const close = () => (dirty ? setConfirmClose(true) : onClose());

  return (
    <div className="fixed inset-0 z-[60] grid place-items-center glass-scrim anim-fade" onClick={close}>
      <div role="dialog" aria-modal="true" aria-label={t("glossary.title")} onClick={(e) => e.stopPropagation()}
        className="w-[min(96vw,1180px)] max-h-[90vh] flex flex-col rounded-xl glass-panel anim-pop">
        <div className="flex items-start gap-3 px-5 pt-4 pb-3 border-b border-[var(--color-border)]">
          <BookA size={18} className="shrink-0 mt-0.5 text-[var(--color-accent)]" />
          <div className="min-w-0 flex-1">
            <h2 className="font-semibold text-[15px]">{t("glossary.title")}</h2>
            <p className="mt-0.5 text-[12px] leading-snug text-[var(--color-muted)]">{t("glossary.hint")}</p>
          </div>
          <button onClick={close} title={t("glossary.close")} className="shrink-0 text-[var(--color-muted)] hover:text-[var(--color-text)]"><X size={16} /></button>
        </div>

        {stale && mode && (
          <div className="mx-5 mt-3 flex flex-wrap items-center gap-2 rounded-lg border border-[var(--color-warn)]/50 bg-[color-mix(in_oklab,var(--color-warn)_10%,transparent)] px-3 py-2 text-[12px]">
            <TriangleAlert size={14} className="shrink-0 text-[var(--color-warn)]" />
            <span className="flex-1 min-w-0">{t("glossary.stale")}</span>
            <button onClick={doRetranslate} disabled={busy !== null} className={BTN}>
              {busy === "retranslate" ? <Loader2 size={13} className="animate-spin" /> : <RotateCw size={13} />}{t("glossary.retranslate")}
            </button>
          </div>
        )}

        <div className="flex flex-wrap items-center gap-1.5 px-5 pt-3">
          <label className="relative flex-1 min-w-[160px] max-w-[280px]">
            <Search size={13} className="absolute left-2 top-1/2 -translate-y-1/2 text-[var(--color-muted)]" />
            <input value={filter} onChange={(e) => setFilter(e.target.value)} placeholder={t("glossary.filter")} aria-label={t("glossary.filter")}
              className={`${CELL} pl-7`} />
          </label>
          <button onClick={() => setRows((rs) => [toRow(blank(lang)), ...(rs || [])])} disabled={rows === null} className={BTN}><Plus size={13} />{t("glossary.add")}</button>
          <button onClick={doExtract} disabled={busy !== null || rows === null} title={t("glossary.extractHint")} className={BTN}>
            {busy === "extract" ? <Loader2 size={13} className="animate-spin" /> : <Sparkles size={13} />}{busy === "extract" ? t("glossary.extracting") : t("glossary.extract")}
          </button>
          <button onClick={() => fileRef.current?.click()} disabled={busy !== null || rows === null} title={t("glossary.importHint")} className={BTN}>
            {busy === "import" ? <Loader2 size={13} className="animate-spin" /> : <Upload size={13} />}{t("glossary.import")}
          </button>
          <input ref={fileRef} type="file" accept=".tsv,.txt,text/tab-separated-values" className="hidden"
            onChange={(e) => { const f = e.target.files?.[0]; if (f) void doImport(f); e.target.value = ""; }} />
          <button onClick={doExport} disabled={busy !== null || rows === null} title={t("glossary.exportHint")} className={BTN}>
            {busy === "export" ? <Loader2 size={13} className="animate-spin" /> : <Download size={13} />}{t("glossary.export")}
          </button>
          <div className="ml-auto inline-flex items-center gap-1.5" title={profiles.length ? t("glossary.seriesHint") : t("glossary.noSeries")}>
            <select value={seriesSlug} onChange={(e) => setSeries(e.target.value)} disabled={!profiles.length || busy !== null} aria-label={t("glossary.series")}
              className="max-w-[180px] bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-md px-1.5 py-1 text-[12px] disabled:opacity-40">
              {!profiles.length && <option value="">{t("glossary.noSeriesShort")}</option>}
              {profiles.map((p) => <option key={p.slug} value={p.slug}>{p.name}</option>)}
            </select>
            <button onClick={doSeries} disabled={!seriesSlug || busy !== null || rows === null} className={BTN}>
              {busy === "series" ? <Loader2 size={13} className="animate-spin" /> : <BookUp size={13} />}{t("glossary.toSeries")}
            </button>
          </div>
        </div>

        {candidates.length > 0 && (
          <div className="mx-5 mt-3 rounded-lg border border-[var(--color-accent)]/50 bg-[color-mix(in_oklab,var(--color-accent)_7%,transparent)] p-2.5">
            <div className="flex items-center gap-2 mb-1.5 text-[12px]">
              <Sparkles size={13} className="text-[var(--color-accent)]" />
              <span className="font-medium flex-1">{t("glossary.candidates", { n: candidates.length })}</span>
              <button onClick={() => accept(candidates)} className={BTN}><Check size={13} />{t("glossary.acceptAll")}</button>
              <button onClick={() => setCandidates([])} className={BTN}><X size={13} />{t("glossary.dismissAll")}</button>
            </div>
            <div className="max-h-[22vh] overflow-y-auto space-y-1">
              {candidates.map((c, i) => (
                <div key={`${c.term}-${i}`} className="flex items-center gap-2 text-[12px] rounded-md bg-[var(--color-surface-2)]/60 px-2 py-1">
                  <span className="font-medium">{c.term}</span>
                  <span className="text-[var(--color-muted)]">→</span>
                  <span className="min-w-0 truncate">{c.keep ? t("glossary.keepShort") : c.translation}</span>
                  {c.note && <span className="min-w-0 truncate text-[11px] text-[var(--color-muted)]">· {c.note}</span>}
                  <span className="ml-auto inline-flex gap-1 shrink-0">
                    <button onClick={() => accept([c])} title={t("glossary.accept")} className="p-0.5 text-[var(--color-muted)] hover:text-[var(--color-accent)]"><Check size={14} /></button>
                    <button onClick={() => setCandidates((cs) => cs.filter((x) => x !== c))} title={t("glossary.dismiss")} className="p-0.5 text-[var(--color-muted)] hover:text-[#ef4444]"><X size={14} /></button>
                  </span>
                </div>
              ))}
            </div>
          </div>
        )}

        <div className="flex-1 min-h-0 overflow-auto px-5 py-3">
          {rows === null ? (
            !error && <div className="py-10 grid place-items-center"><Loader2 size={18} className="animate-spin text-[var(--color-accent)]" /></div>
          ) : rows.length === 0 ? (
            <div className="py-10 text-center text-[12px] text-[var(--color-muted)]">{t("glossary.empty")}</div>
          ) : (
            <table className="w-full border-separate border-spacing-y-1 text-[12px]">
              <thead>
                <tr className="text-left text-[10px] uppercase tracking-wider text-[var(--color-muted)]">
                  <th className="font-medium px-1 w-[16%]">{t("glossary.colTerm")}</th>
                  <th className="font-medium px-1 w-[17%]">{t("glossary.colTranslation")}</th>
                  <th className="font-medium px-1 w-[6%] text-center" title={t("glossary.colKeepHint")}>{t("glossary.colKeep")}</th>
                  <th className="font-medium px-1 w-[15%]" title={t("glossary.colPronunciationHint")}>{t("glossary.colPronunciation")}</th>
                  <th className="font-medium px-1 w-[18%]" title={t("glossary.colAsrHint")}>{t("glossary.colAsr")}</th>
                  <th className="font-medium px-1">{t("glossary.colNote")}</th>
                  <th className="w-8" />
                </tr>
              </thead>
              <tbody>
                {shown.map((r) => (
                  <tr key={r.key} className="align-middle">
                    <td className="px-1">
                      <div className="flex items-center gap-1">
                        <input value={r.entry.term} onChange={(e) => edit(r.key, { term: e.target.value })} aria-label={t("glossary.colTerm")} className={CELL} />
                        {r.entry.source === "auto" && (
                          <span title={t("glossary.autoHint")} className="shrink-0 rounded px-1 py-px text-[9px] uppercase bg-[var(--color-accent)]/20 text-[var(--color-accent)]">{t("glossary.auto")}</span>
                        )}
                      </div>
                    </td>
                    <td className="px-1">
                      <input value={r.entry.translation} disabled={r.entry.keep} onChange={(e) => edit(r.key, { translation: e.target.value })}
                        aria-label={t("glossary.colTranslation")} className={`${CELL} disabled:opacity-40`} />
                    </td>
                    <td className="px-1 text-center">
                      <input type="checkbox" checked={r.entry.keep} onChange={(e) => edit(r.key, { keep: e.target.checked })}
                        aria-label={t("glossary.colKeepHint")} className="accent-[var(--color-accent)]" />
                    </td>
                    <td className="px-1">
                      <input value={r.entry.pronunciation} onChange={(e) => edit(r.key, { pronunciation: e.target.value })} aria-label={t("glossary.colPronunciation")} className={CELL} />
                    </td>
                    <td className="px-1">
                      <input value={r.asr} onChange={(e) => edit(r.key, {}, e.target.value)} aria-label={t("glossary.colAsr")} className={CELL} />
                    </td>
                    <td className="px-1">
                      <input value={r.entry.note} onChange={(e) => edit(r.key, { note: e.target.value })} aria-label={t("glossary.colNote")} className={CELL} />
                    </td>
                    <td className="px-1 text-center">
                      <button onClick={() => setRows((rs) => (rs || []).filter((x) => x.key !== r.key))} title={t("glossary.remove")}
                        className="p-1 text-[var(--color-muted)] hover:text-[#ef4444] transition-colors"><Trash2 size={13} /></button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>

        <div className="flex flex-wrap items-center gap-2 px-5 py-3 border-t border-[var(--color-border)] text-[12px]">
          <span className="text-[var(--color-muted)]">{t("glossary.count", { n: entries.filter((e) => e.term.trim()).length })}</span>
          {dirty && <span className="text-[var(--color-accent)]">● {t("glossary.unsaved")}</span>}
          {error && <span role="alert" className="min-w-0 flex-1 mono text-[11px] text-[var(--color-warn)] break-words">{error}</span>}
          {notice && !error && <span className="min-w-0 flex-1 text-[var(--color-muted)] break-words">{notice}</span>}
          <div className="ml-auto flex gap-2">
            <button onClick={close} className="px-3.5 py-1.5 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-2)] text-[13px] hover:border-[#3a414c] transition-colors">{t("glossary.close")}</button>
            <button onClick={doSave} disabled={!dirty || busy !== null}
              className="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg text-[13px] font-semibold bg-[var(--color-accent)] text-[var(--color-on-accent)] hover:brightness-105 disabled:opacity-50 transition">
              {busy === "save" && <Loader2 size={13} className="animate-spin" />}{t("glossary.save")}
            </button>
          </div>
        </div>
      </div>
      {confirmClose && (
        <ConfirmDialog title={t("glossary.unsavedTitle")} message={t("glossary.unsavedMessage")} confirmLabel={t("glossary.discard")} danger
          onConfirm={onClose} onCancel={() => setConfirmClose(false)} />
      )}
    </div>
  );
}

// Кнопка глоссария для редактора и транскрипта: открывает панель поверх экрана.
export function GlossaryButton({ pid, project, wide = false }: { pid: string; project: Project; wide?: boolean }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  return (
    <>
      <button onClick={() => setOpen(true)} title={t("glossary.openHint")}
        className={wide
          ? "flex-1 inline-flex items-center justify-center gap-1.5 px-3 py-2 rounded-lg border border-[#37414d] text-[12px] hover:border-[var(--color-accent)]"
          : "inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-[11px] bg-[var(--color-surface-2)] border border-[var(--color-border)] text-[var(--color-text)] hover:border-[var(--color-accent)] hover:text-[var(--color-accent)] transition-colors shrink-0"}>
        <BookA size={wide ? 13 : 12} />{t("glossary.open")}
      </button>
      {open && createPortal(<GlossaryPanel pid={pid} project={project} onClose={() => setOpen(false)} />, document.body)}
    </>
  );
}
