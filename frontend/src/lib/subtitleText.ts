import type { Segment } from "./api";

// Текст субтитра строки в режиме субтитров проекта — как его вжигает сервер (subs_text.rs): в дубляже
// «оригинал» — src_text (tgt там — озвученный перевод), двуязычный — перевод и оригинал двумя строками.
export function subtitleText(s: Pick<Segment, "src_text" | "tgt_text">, mode: string, isDub: boolean, originalTop: boolean): string {
  const tgt = (s.tgt_text || "").trim();
  const src = (s.src_text || "").trim();
  if (mode === "transcribe") return isDub ? src : tgt || src;
  if (mode !== "bilingual") return tgt || src;
  if (!tgt) return src;
  if (!src || src === tgt) return tgt;
  return originalTop ? `${src}\n${tgt}` : `${tgt}\n${src}`;
}
