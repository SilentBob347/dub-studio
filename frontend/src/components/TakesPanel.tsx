import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, Loader2, Pause, Pin, PinOff, Play, RotateCcw } from "lucide-react";
import { api, type Segment, type Takes } from "../lib/api";

// История дублей фразы у строки редактора: прослушать каждый дубль, выбрать активный (A/B), откатить к
// старому (дубль другого текста возвращает и текст), закрепить активный. Выбор и закрепление делает
// редактор (onSelect/onPin): им нужны undo, журнал и пересборка микса.
export default function TakesPanel({ pid, seg, disabled, onSelect, onPin }: {
  pid: string;
  seg: Segment;
  disabled: boolean;
  onSelect: (n: number) => Promise<void>;
  onPin: (pinned: boolean) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [takes, setTakes] = useState<Takes | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [playing, setPlaying] = useState<number | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const audio = useRef<HTMLAudioElement | null>(null);
  const summary = seg.takes;

  useEffect(() => {
    let live = true;
    api.takes(pid, seg.id)
      .then((r) => { if (live) { setTakes(r); setError(null); } })
      .catch((e: unknown) => { if (live) setError(e instanceof Error ? e.message : String(e)); });
    return () => { live = false; };
  }, [pid, seg.id, seg.tgt_text, summary?.count, summary?.active, summary?.pinned]);

  useEffect(() => () => { audio.current?.pause(); }, []);

  function toggle(n: number) {
    const cur = audio.current;
    if (playing === n) { cur?.pause(); setPlaying(null); return; }
    cur?.pause();
    const a = new Audio(api.takeAudioUrl(pid, seg.id, n));
    audio.current = a;
    a.onended = () => setPlaying(null);
    a.play().then(() => setPlaying(n), (e: unknown) => { setPlaying(null); setError(e instanceof Error ? e.message : String(e)); });
  }

  async function run(tag: string, act: () => Promise<void>) {
    setBusy(tag);
    try { await act(); } finally { setBusy(null); }
  }

  if (error) return <div className="mt-1.5 mono text-[11px] text-[var(--color-warn)] break-words">{t("takes.loadFailed", { error })}</div>;
  if (!takes) return <div className="mt-1.5 text-[11px] text-[var(--color-muted)] inline-flex items-center gap-1"><Loader2 size={11} className="animate-spin" />{t("takes.loading")}</div>;
  if (!takes.takes.length) return <div className="mt-1.5 text-[11px] text-[var(--color-muted)]">{t("takes.empty")}</div>;
  const btn = "inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[11px] border border-[var(--color-border)] bg-[var(--color-surface-2)] text-[var(--color-muted)] hover:text-[var(--color-accent)] hover:border-[var(--color-accent)] disabled:opacity-40 transition-colors";
  return (
    <div className="mt-1.5 rounded-lg border border-[var(--color-border)] bg-[var(--color-bg)]/40 divide-y divide-[var(--color-border)]" onClick={(e) => e.stopPropagation()}>
      {takes.takes.map((tk) => {
        const active = takes.active === tk.n;
        const pinned = takes.pinned === tk.n;
        return (
          <div key={tk.n} className={`flex items-start gap-1.5 px-1.5 py-1 ${active ? "bg-[color-mix(in_oklab,var(--color-accent)_8%,transparent)]" : ""}`}>
            <button onClick={() => toggle(tk.n)} title={playing === tk.n ? t("common.pause") : t("takes.listen")}
              className="p-0.5 mt-px text-[var(--color-muted)] hover:text-[var(--color-accent)] transition-colors shrink-0">
              {playing === tk.n ? <Pause size={12} /> : <Play size={12} />}
            </button>
            <div className="flex-1 min-w-0">
              <div className="flex flex-wrap items-center gap-1 text-[10.5px] text-[var(--color-muted)]">
                <span className="mono tabnum">{t("takes.dur", { sec: tk.dur.toFixed(2) })}</span>
                <span>{t(`takes.source.${tk.source}`)}</span>
                {tk.qc != null && <span className="mono">{t("takes.qc", { pct: Math.round(tk.qc * 100) })}</span>}
                {active && <span className="inline-flex items-center gap-0.5 text-[var(--color-accent)]"><Check size={10} />{t("takes.active")}</span>}
                {pinned && <span className="inline-flex items-center gap-0.5 text-[var(--color-accent)]"><Pin size={10} />{t("takes.pinned")}</span>}
              </div>
              {!tk.text_matches && <div className="text-[11px] leading-snug text-[var(--color-text)]/80 break-words">{tk.text}</div>}
            </div>
            {active ? (
              <button onClick={() => run(`pin${tk.n}`, () => onPin(!pinned))} disabled={disabled || busy !== null || (!pinned && !tk.text_matches)}
                title={pinned ? t("takes.unpinTip") : t("takes.pinTip")} className={btn}>
                {busy === `pin${tk.n}` ? <Loader2 size={11} className="animate-spin" /> : pinned ? <PinOff size={11} /> : <Pin size={11} />}
                {pinned ? t("takes.unpin") : t("takes.pin")}
              </button>
            ) : (
              <button onClick={() => run(`use${tk.n}`, () => onSelect(tk.n))} disabled={disabled || busy !== null}
                title={tk.text_matches ? t("takes.useTip") : t("takes.rollbackTip")} className={btn}>
                {busy === `use${tk.n}` ? <Loader2 size={11} className="animate-spin" /> : tk.text_matches ? <Check size={11} /> : <RotateCcw size={11} />}
                {tk.text_matches ? t("takes.use") : t("takes.rollback")}
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}
