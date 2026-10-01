import { useTranslation } from "react-i18next";
import type { Fit } from "../lib/api";

const TONE = {
  fits: "text-emerald-400 bg-emerald-400/10",
  tight: "text-[var(--color-warn)] bg-[color-mix(in_oklab,var(--color-warn)_12%,transparent)]",
  impossible: "text-[#ef4444] bg-[#ef4444]/10",
} as const;

const x2 = (v: number) => v.toFixed(2);

// Бейдж «влезет / на грани / не влезет» у реплики дубляжа; после рендера — сколько реально пришлось ускорять.
export default function FitBadge({ fit }: { fit: Fit | null | undefined }) {
  const { t } = useTranslation();
  if (!fit) return null;
  const cls = "mono text-[9.5px] px-1 py-0.5 rounded shrink-0 whitespace-nowrap";
  const r = fit.rendered;
  if (r) {
    const tone = r.over ? "impossible" : r.needed > 1 ? "tight" : "fits";
    const label = r.over ? t("fit.renderedOver", { x: x2(r.needed) }) : t(`fit.${tone}`);
    return (
      <span title={t("fit.renderedTip", { raw: x2(r.raw), slot: x2(r.slot), needed: x2(r.needed), cap: x2(r.cap), max: x2(r.eff_cap) })} className={`${cls} ${TONE[tone]}`}>
        {label}
      </span>
    );
  }
  const pace = t(fit.calibrated ? "fit.paceVoice" : "fit.paceLang", { cps: fit.cps.toFixed(1) });
  return (
    <span title={t("fit.predictTip", { est: x2(fit.est), slot: x2(fit.slot), ratio: x2(fit.ratio), cap: x2(fit.cap), max: x2(fit.eff_cap), pace })}
      className={`${cls} ${TONE[fit.verdict]}`}>
      {t(`fit.${fit.verdict}`)}
    </span>
  );
}
