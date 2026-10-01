import { useTranslation } from "react-i18next";

export default function SubsAlignToggle({ checked, onChange }: { checked: boolean; onChange: (v: boolean) => void }) {
  const { t } = useTranslation();
  return (
    <label className="inline-flex items-center gap-1.5 text-[10px] text-[var(--color-muted)] cursor-pointer" title={t("subalign.hint")}>
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} className="accent-[var(--color-accent)]" />
      {t("subalign.label")}
    </label>
  );
}
