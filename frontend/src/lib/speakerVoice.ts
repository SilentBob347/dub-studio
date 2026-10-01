import i18n from "./i18n";
import { api, ApiError } from "./api";
import { useStore } from "../store";

// Причина отказа «Сделать голос» языком окна по коду сервера (подробность сервера — в тексте).
export function speakerVoiceErrorText(spk: string, e: unknown): string {
  if (e instanceof ApiError && e.code === "no_separation") {
    return i18n.t("voice.errNoSeparation", { spk, settings: i18n.t("prefs.title"), models: i18n.t("prefs.sections.models"), section: i18n.t("settings.roleSep") });
  }
  if (e instanceof ApiError && e.code === "separation_failed") return i18n.t("voice.errSeparation", { spk, detail: e.detail });
  if (e instanceof ApiError && e.code === "no_speaker_lines") return i18n.t("voice.errNoLines", { spk });
  const detail = e instanceof ApiError ? e.detail || e.code : e instanceof Error ? e.message : String(e);
  return i18n.t("voice.errFailed", { spk, detail });
}

// «Сделать голос» из спикера: ответ сервера, либо null и причина строкой-ошибкой в журнале окна.
export async function makeSpeakerVoice(pid: string, spk: string, name: string): Promise<{ name: string; voices: string[] } | null> {
  try {
    return await api.speakerVoice(pid, spk, name);
  } catch (e) {
    useStore.getState().pushActivity(speakerVoiceErrorText(spk, e), "error");
    return null;
  }
}
