import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { Languages, Music } from "lucide-react";
import { DUB_LANGS, LANGS, setLang, type Lang } from "../../lib/i18n";
import { playSfx, setSfxEnabled, sfxEnabled } from "../../lib/sfx";
import SettingSwitch from "./SettingSwitch";

const langName = (code: Lang) => DUB_LANGS.find((l) => l.code === code)?.name ?? code;

export default function InterfaceSection() {
  const { t, i18n } = useTranslation();
  const [sfx, setSfx] = useState(sfxEnabled());
  const selectId = useId();
  return (
    <div className="max-w-2xl rounded-lg border border-[var(--color-border)] divide-y divide-[var(--color-border)] bg-[var(--color-surface-2)]/40">
      <div className="flex items-center gap-3 px-3 py-2.5">
        <Languages size={15} className="shrink-0 text-[var(--color-accent-2)]" />
        <label htmlFor={selectId} className="min-w-0 flex-1">
          <span className="block text-[13px] font-medium">{t("prefs.uiLanguage")}</span>
          <span className="block text-[11px] leading-snug text-[var(--color-muted)]">{t("prefs.uiLanguageHint")}</span>
        </label>
        <select id={selectId} value={i18n.language as Lang} onChange={(e) => setLang(e.target.value as Lang)}
          className="shrink-0 bg-[var(--color-surface)] border border-[var(--color-border)] rounded-md px-2 py-1 text-[12px] focus:border-[var(--color-accent)] focus:outline-none">
          {LANGS.map((l) => <option key={l} value={l}>{langName(l)}</option>)}
        </select>
      </div>
      <SettingSwitch icon={Music} label={t("settings.sounds")} hint={t("prefs.soundsHint")} on={sfx}
        onToggle={() => { const v = !sfx; setSfx(v); setSfxEnabled(v); if (v) playSfx("notify"); }} />
    </div>
  );
}
