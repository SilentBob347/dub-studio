// Язык субтитров проекта отдельно от аудио: нет / оригинал / перевод / оба. У двуязычных — порядок строк
// и вид второй строки (оригинала): размер от основной, свой цвет и непрозрачность; null — как у основной.
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { RotateCcw, SlidersHorizontal } from "lucide-react";
import type { Project } from "../lib/api";

type Props = {
  subs: Project["subs"];
  primaryColor: string;
  onPatch: (op: string, extra: Record<string, unknown>) => Promise<unknown>;
};

export default function SubsContentControl({ subs, primaryColor, onPatch }: Props) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const bi = subs.bilingual;
  const sec = bi.secondary;
  // Черновик ползунка, пока его тянут; сохраняется, когда отпустили.
  const [sizeDraft, setSizeDraft] = useState<number | null>(null);
  const [opacityDraft, setOpacityDraft] = useState<number | null>(null);
  const size = sizeDraft ?? sec.size_pct;
  const opacity = opacityDraft ?? sec.opacity ?? 100;
  const bilingual = subs.mode === "bilingual";
  const secondary = (patch: Record<string, unknown>) => onPatch("subs_content", { secondary: patch });
  const commitSize = () => {
    if (sizeDraft == null) return;
    if (sizeDraft === sec.size_pct) { setSizeDraft(null); return; }
    void secondary({ size_pct: sizeDraft }).then(() => setSizeDraft(null));
  };
  const commitOpacity = () => {
    if (opacityDraft == null) return;
    if (opacityDraft === sec.opacity) { setOpacityDraft(null); return; }
    void secondary({ opacity: opacityDraft }).then(() => setOpacityDraft(null));
  };
  const chip = (on: boolean) =>
    `flex-1 px-2 py-1 rounded-md text-[11px] border transition-colors ${on ? "border-[var(--color-accent)] bg-[color-mix(in_oklab,var(--color-accent)_12%,transparent)] text-[var(--color-text)]" : "border-[var(--color-border)] text-[var(--color-muted)] hover:text-[var(--color-text)]"}`;

  return (
    <div className="relative flex items-center gap-1.5" data-subs-content>
      <select value={subs.mode} onChange={(e) => onPatch("subs_content", { value: e.target.value })} aria-label={t("comp.subsLabel")}
        className="bg-[var(--color-surface-2)] border border-[var(--color-border)] rounded-md px-2 py-1 text-[12px] text-[var(--color-text)] focus:border-[var(--color-accent)] focus:outline-none">
        <option value="none">{t("comp.subsNone")}</option>
        <option value="transcribe">{t("comp.subsOriginal")}</option>
        <option value="translate">{t("comp.subsTranslate")}</option>
        <option value="bilingual">{t("comp.subsBilingual")}</option>
      </select>
      {bilingual && (
        <button onClick={() => setOpen((v) => !v)} title={t("bilingual.settings")} aria-label={t("bilingual.settings")} aria-expanded={open}
          className={`p-1.5 rounded-md border transition-colors ${open ? "border-[var(--color-accent)] text-[var(--color-accent)]" : "border-[var(--color-border)] text-[var(--color-muted)] hover:text-[var(--color-text)]"}`}>
          <SlidersHorizontal size={13} />
        </button>
      )}
      {bilingual && open && (
        <div className="absolute left-0 top-full mt-2 z-30 w-72 rounded-xl border border-[var(--color-border)] bg-[var(--color-surface)] shadow-xl p-3 space-y-3"
          onKeyDown={(e) => { if (e.key === "Escape") setOpen(false); }}>
          <div className="text-[12px] font-semibold">{t("bilingual.title")}</div>
          <div className="flex gap-1.5">
            <button onClick={() => onPatch("subs_content", { order: "translation_top" })} className={chip(bi.order === "translation_top")}>{t("bilingual.translationTop")}</button>
            <button onClick={() => onPatch("subs_content", { order: "original_top" })} className={chip(bi.order === "original_top")}>{t("bilingual.originalTop")}</button>
          </div>
          <label className="block">
            <div className="flex justify-between text-[11px] text-[var(--color-muted)] mb-1">
              <span>{t("bilingual.size")}</span><span className="mono">{t("bilingual.sizeValue", { pct: size })}</span>
            </div>
            <input type="range" min={40} max={100} step={5} value={size} onChange={(e) => setSizeDraft(parseInt(e.target.value))}
              onPointerUp={commitSize} onKeyUp={commitSize} onBlur={commitSize}
              className="w-full accent-[var(--color-accent)]" />
          </label>
          <div className="flex items-center gap-2">
            <span className="text-[11px] text-[var(--color-muted)] flex-1">{t("bilingual.color")}</span>
            <input type="color" value={sec.color ?? primaryColor} onChange={(e) => secondary({ color: e.target.value })} title={t("bilingual.color")}
              className="w-8 h-6 rounded bg-transparent cursor-pointer border border-[var(--color-border)]" />
            <button onClick={() => secondary({ color: null })} disabled={sec.color == null} title={t("bilingual.sameAsMain")} aria-label={t("bilingual.sameAsMain")}
              className="p-1 rounded text-[var(--color-muted)] hover:text-[var(--color-accent)] disabled:opacity-30 transition-colors"><RotateCcw size={12} /></button>
          </div>
          <label className="block">
            <div className="flex justify-between items-center text-[11px] text-[var(--color-muted)] mb-1">
              <span>{t("bilingual.opacity")}</span>
              <span className="flex items-center gap-1.5">
                <span className="mono">{sec.opacity == null && opacityDraft == null ? t("bilingual.sameAsMain") : t("bilingual.pct", { pct: opacity })}</span>
                <button onClick={(e) => { e.preventDefault(); secondary({ opacity: null }); }} disabled={sec.opacity == null} title={t("bilingual.sameAsMain")} aria-label={t("bilingual.sameAsMain")}
                  className="p-0.5 rounded hover:text-[var(--color-accent)] disabled:opacity-30 transition-colors"><RotateCcw size={11} /></button>
              </span>
            </div>
            <input type="range" min={10} max={100} step={5} value={opacity} onChange={(e) => setOpacityDraft(parseInt(e.target.value))}
              onPointerUp={commitOpacity} onKeyUp={commitOpacity} onBlur={commitOpacity}
              className="w-full accent-[var(--color-accent)]" />
          </label>
        </div>
      )}
    </div>
  );
}
