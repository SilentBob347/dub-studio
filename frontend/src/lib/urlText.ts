import { useTranslation } from "react-i18next";

// Тексты по кодам сервера: ошибка, предупреждение о субтитрах, фаза загрузки.
export function useUrlText() {
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
