// Правка текста заменила подряд идущие слова (до 4 с каждой стороны), остальное осталось: было -> стало.
// Так правка распознанной фразы превращается в предложение записать термин в глоссарий.
const clean = (w: string) => w.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "");
const key = (w: string) => clean(w).toLowerCase().normalize("NFD").replace(/\u0308/g, "").normalize("NFC");

export function wordSwap(before: string, after: string): { from: string; to: string } | null {
  const a = before.split(/\s+/).filter(Boolean);
  const b = after.split(/\s+/).filter(Boolean);
  let head = 0;
  while (head < a.length && head < b.length && key(a[head]) === key(b[head])) head++;
  let tail = 0;
  while (tail < a.length - head && tail < b.length - head && key(a[a.length - 1 - tail]) === key(b[b.length - 1 - tail])) tail++;
  const from = a.slice(head, a.length - tail).map(clean).filter(Boolean);
  const to = b.slice(head, b.length - tail).map(clean).filter(Boolean);
  if (!from.length || !to.length || from.length > 4 || to.length > 4) return null;
  return { from: from.join(" "), to: to.join(" ") };
}
