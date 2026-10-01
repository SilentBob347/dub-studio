// editor stages mapped to the engine's stage markers (api._run emits `stage` per _timed block + "download").
export type AnalyzeStepKey = "download" | "separating" | "diarizing" | "recognizing" | "translating" | "voicing" | "locating" | "casting" | "assembling";
export const ANALYZE_STEPS: { key: AnalyzeStepKey; stages: string[] }[] = [
  { key: "download",    stages: ["download"] },
  { key: "separating",  stages: ["extract_audio", "separate"] },
  { key: "diarizing",   stages: ["diarize"] },
  { key: "recognizing", stages: ["asr"] },
  { key: "translating", stages: ["translate", "translate_ctx", "vision", "rewrite", "rewrite_ctx"] },   // "vision" = ctx-проход (vision layout + перевод транскрипта)
  { key: "voicing",     stages: ["tts", "mix"] },        // TTS synthesis + mix — runs BETWEEN translate and OCR; without this the stepper blanks (cur=-1) during voice gen
  // «Находим текст на экране» = ТОЛЬКО OCR-стадии: юзер с выключенной детекцией не должен видеть этот
  // шаг вовсе (жалоба). Сборка выходного файла (build/burn/mux) — отдельный честный шаг.
  { key: "locating",    stages: ["ocr_detect", "translate_titles", "translate_tagline"] },
  { key: "casting",     stages: ["cast_detect", "cast_embed", "cast_speaker"] },   // #115: лица (SCRFD) + эмбеддинги (LVFace) + active-speaker (LR-ASD)
  { key: "assembling",  stages: ["build", "burn", "mux"] },
];

// стадия -> ключ шага (метка шага — t(`analyze.${key}`)). Неизвестная стадия -> undefined.
export const STAGE_TO_STEPKEY: Record<string, AnalyzeStepKey | undefined> = Object.fromEntries(
  ANALYZE_STEPS.flatMap((s) => s.stages.map((st) => [st, s.key])),
);
