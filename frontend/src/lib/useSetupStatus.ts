import { useCallback, useEffect, useRef, useState } from "react";
import { api, type DownloadJob, type SetupStatus } from "./api";

// Статус «Первого запуска» с фоновой закачкой: пока закачка идёт, опрашиваем /setup/status раз в секунду
// (прогресс, скорость, ожидание сервера); onSettled — когда идущая закачка завершилась, встала на паузу или упала.
export function useSetupStatus(onSettled?: (job: DownloadJob, status: SetupStatus) => void) {
  const [status, setStatus] = useState<SetupStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const wasRunning = useRef(false);
  const settled = useRef(onSettled);
  useEffect(() => { settled.current = onSettled; }, [onSettled]);

  const apply = useCallback((s: SetupStatus) => {
    setStatus(s);
    setError(null);
    const running = s.active?.status === "downloading";
    if (wasRunning.current && !running && s.active) settled.current?.(s.active, s);
    wasRunning.current = running;
    return s;
  }, []);
  const fail = useCallback((e: unknown) => { setError(e instanceof Error ? e.message : String(e)); }, []);
  // Ошибка запроса и показывается (error), и отдаётся вызывающему.
  const refresh = useCallback(() => api.setupStatus().then(apply, (e: unknown) => { fail(e); throw e; }), [apply, fail]);

  const downloading = status?.active?.status === "downloading";
  useEffect(() => {
    const load = () => { api.setupStatus().then(apply, fail); };
    load();
    if (!downloading) return;
    const id = setInterval(load, 1000);
    return () => clearInterval(id);
  }, [downloading, apply, fail]);

  // Статус пришёл из ответа другого маршрута (удаление/импорт) — принимаем его без перехода закачки.
  const adopt = useCallback((s: SetupStatus) => {
    setStatus(s);
    wasRunning.current = s.active?.status === "downloading";
  }, []);

  return { status, error, refresh, adopt, downloading };
}
