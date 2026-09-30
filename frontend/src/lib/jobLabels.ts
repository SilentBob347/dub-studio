// Тексты статуса джобы проекта: состояние + шаг остановки и человеческая ошибка по коду.
import { useTranslation } from "react-i18next";
import type { JobErrorInfo } from "./api";
import { STAGE_TO_STEPKEY } from "./stages";

// Текст ошибки джобы по коду (перевод) + исходное сообщение движка для «Подробностей».
export function useJobErrorText() {
  const { t } = useTranslation();
  return (err: JobErrorInfo | null | undefined): string | undefined => {
    if (!err) return undefined;
    const human = t(`jobs.error.${err.code}`, { defaultValue: t("jobs.error.failed") });
    return err.text ? `${human}: ${err.text}` : human;
  };
}

// Метка «Прервано · Перевод» для джобы проекта (состояние + шаг, на котором она остановилась).
export function useJobStateLabel() {
  const { t } = useTranslation();
  return (state: string, stage: string | null | undefined): string => {
    const stateText = t(`jobs.state.${state}`);
    const step = stage ? STAGE_TO_STEPKEY[stage] : undefined;
    return step ? t("jobs.atStage", { state: stateText, stage: t(`analyze.${step}`) }) : stateText;
  };
}
