//! Выравнивание импортированных SRT/ASS по речи: текст реплик остаётся из файла, тайминги берутся у
//! распознанных слов. Основа — align_lyrics студий (YuE2-Studio lyrics_sync.rs): все реплики ставятся
//! разом монотонным DP по максимуму суммарного совпадения (окна длиной span-1..span+2, порог 0.45,
//! полный зачёт от 0.7, при равенстве — раньше, донастройка ±2 слова), совпадение — посимвольный Dice
//! студии (normalise дословно, dice/similarity — эталон в тестах), поэтому недослышанные слова не рушат
//! реплику, а повторы («Да!») расходуются по порядку.
//!
//! Адаптация под эпизод: студийный DP O(L·W²) и матрица оценок O(L·W) на серии в 400 реплик и 4000 слов
//! неподъёмны, поэтому кандидаты — только слова в окне ±`WINDOW_SECS` вокруг начала реплики со сдвигом
//! всего файла (оценка по редким совпавшим словам), а DP идёт по префиксному максимуму. Конец реплики —
//! конец последнего совпавшего слова (у студии слово без конца), не дальше начала следующей. Реплика без
//! совпадения сдвигается смещением соседей с сохранением длительности, а не растягивается.

use std::collections::HashMap;

/// Окно поиска кандидатов вокруг начала реплики (со сдвигом файла), сек.
const WINDOW_SECS: f64 = 30.0;
/// Ниже этой доли сопоставленных реплик субтитры не про эту речь (другой релиз с другим текстом, другой
/// язык) — тайминги файла оставляем как есть.
pub const MIN_ALIGNED_SHARE: f64 = 0.3;
/// Минимальная длительность реплики после выравнивания, сек.
const MIN_DUR: f64 = 0.3;

/// Услышанное слово: текст, начало, конец (сек).
pub type Heard = (String, f64, f64);

/// Как реплика получила тайминг.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    /// Совпала с распознанной речью: начало первого и конец последнего совпавшего слова.
    Aligned,
    /// Не совпала: сдвинута смещением соседних совпавших реплик, длительность сохранена.
    Shifted,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub start: f64,
    pub end: f64,
    pub how: How,
}

/// Итог выравнивания файла.
#[derive(Debug, Clone)]
pub struct Alignment {
    /// По реплике на каждый cue, в исходном порядке.
    pub cues: Vec<Placed>,
    /// Сдвиг всего файла (речь минус субтитры), сек.
    pub offset: f64,
    /// Доля сопоставленных реплик.
    pub share: f64,
}

fn normalise(word: &str) -> String {
    word.chars().filter(|character| character.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Лучший Dice реплики `written` против окон heard[start..start+len], len = span-1..span+2; возвращает
/// (оценка, длина окна). Dice и посимвольный LCS — студийные dice/similarity (их дословная копия в
/// тестах — эталон); один проход LCS по самому длинному окну даёт LCS для всех более коротких (строка
/// DP по префиксу услышанного), поэтому окна не пересчитываются по отдельности.
fn best_window(written: &[char], heard: &[(String, f64, f64)], start: usize, span: usize) -> (f64, usize) {
    let lo = span.saturating_sub(1).max(1);
    let hi = (span + 2).min(heard.len() - start);
    if hi < lo {
        return (0.0, 0);
    }
    let mut spoken: Vec<char> = Vec::new();
    let mut bounds: Vec<usize> = Vec::with_capacity(hi);
    for (w, _, _) in &heard[start..start + hi] {
        spoken.extend(w.chars());
        bounds.push(spoken.len());
    }
    let mut previous = vec![0usize; spoken.len() + 1];
    let mut current = vec![0usize; spoken.len() + 1];
    for &l in written {
        for r in 0..spoken.len() {
            current[r + 1] = if l == spoken[r] { previous[r] + 1 } else { current[r].max(previous[r + 1]) };
        }
        std::mem::swap(&mut previous, &mut current);
        current.iter_mut().for_each(|v| *v = 0);
    }
    let left = written.len();
    let mut best = (0.0f64, 0usize);
    for len in lo..=hi {
        let right = bounds[len - 1];
        if left + right == 0 {
            continue;
        }
        let d = 2.0 * previous[right] as f64 / (left + right) as f64;
        if d > best.0 {
            best = (d, len);
        }
    }
    best
}

/// Грубый сдвиг файла для центрирования окна поиска: каждое слово реплики длиной от 4 букв, встреченное
/// в речи не больше 3 раз, голосует за (время слова − его ожидаемое время в реплике по доле символов);
/// пик гистограммы по 0.5 с — сдвиг. Нет голосов — 0.
fn estimate_offset(cues: &[(f64, f64, Vec<String>)], heard: &[(String, f64, f64)]) -> f64 {
    let mut index: HashMap<&str, Vec<f64>> = HashMap::new();
    for (w, s, _) in heard {
        if w.chars().count() >= 4 {
            index.entry(w.as_str()).or_default().push(*s);
        }
    }
    let mut votes: HashMap<i64, (usize, f64)> = HashMap::new();
    for (start, end, tokens) in cues {
        let total: usize = tokens.iter().map(|t| t.chars().count()).sum::<usize>().max(1);
        let mut before = 0usize;
        for t in tokens {
            let expected = start + (end - start).max(0.0) * before as f64 / total as f64;
            before += t.chars().count();
            if t.chars().count() < 4 {
                continue;
            }
            let Some(times) = index.get(t.as_str()) else { continue };
            if times.len() > 3 {
                continue;
            }
            for &at in times {
                let d = at - expected;
                let e = votes.entry((d / 0.5).round() as i64).or_insert((0, 0.0));
                e.0 += 1;
                e.1 += d;
            }
        }
    }
    votes
        .into_iter()
        .max_by(|a, b| a.1 .0.cmp(&b.1 .0).then(b.0.abs().cmp(&a.0.abs())))
        .map(|(_, (n, sum))| sum / n as f64)
        .unwrap_or(0.0)
}

/// Выровнять реплики (start, end, text) по услышанным словам. Возвращает None, если слов нет.
pub fn align(cues: &[(f64, f64, &str)], words: &[Heard]) -> Option<Alignment> {
    let heard: Vec<(String, f64, f64)> = words
        .iter()
        .map(|(w, s, e)| (normalise(w), *s, e.max(*s)))
        .filter(|(w, _, _)| !w.is_empty())
        .collect();
    if heard.is_empty() || cues.is_empty() {
        return None;
    }
    let count = heard.len();
    let tokens: Vec<(f64, f64, Vec<String>)> = cues
        .iter()
        .map(|(s, e, t)| (*s, *e, t.split_whitespace().map(normalise).filter(|w| !w.is_empty()).collect()))
        .collect();
    let window_offset = estimate_offset(&tokens, &heard);

    // Оценки кандидатов: только слова в окне вокруг ожидаемого начала реплики.
    let mut scores: Vec<HashMap<usize, (f64, usize)>> = Vec::with_capacity(cues.len());
    for (start, _, toks) in &tokens {
        let span = toks.len();
        let mut row: HashMap<usize, (f64, usize)> = HashMap::new();
        if span > 0 {
            let written: Vec<char> = toks.concat().chars().collect();
            let lo = start + window_offset - WINDOW_SECS;
            let hi = start + window_offset + WINDOW_SECS;
            let first = heard.partition_point(|(_, s, _)| *s < lo);
            for j in first..count {
                if heard[j].1 > hi {
                    break;
                }
                let (score, len) = best_window(&written, &heard, j, span);
                if score >= 0.45 {
                    row.insert(j, (score, len));
                }
            }
        }
        scores.push(row);
    }

    // DP студии: best[p] — лучшее (сумма зачётов, сумма стартов) при следующей реплике не раньше слова
    // p; реплика либо ставится на старт ≥ p, либо пропускается. Префиксный максимум вместо перебора всех p.
    let credit = |score: f64| if score >= 0.7 { 1.0 } else { score };
    let better = |left: (f64, usize), right: (f64, usize)| left.0 > right.0 + 1e-9 || ((left.0 - right.0).abs() <= 1e-9 && left.1 < right.1);
    let mut best: Vec<Option<(f64, usize)>> = vec![None; count + 1];
    best[0] = Some((0.0, 0));
    // Разреженные обратные ссылки: позиция -> (откуда, старт). Нет записи — реплика пропущена.
    let mut back: Vec<HashMap<usize, (usize, usize)>> = Vec::with_capacity(cues.len());
    for (index, (_, _, toks)) in tokens.iter().enumerate() {
        let span = toks.len();
        let mut prefix: Vec<Option<((f64, usize), usize)>> = vec![None; count + 1];
        let mut run: Option<((f64, usize), usize)> = None;
        for p in 0..=count {
            if let Some(v) = best[p] {
                if run.is_none_or(|(kept, _)| better(v, kept)) {
                    run = Some((v, p));
                }
            }
            prefix[p] = run;
        }
        let mut next = best.clone();
        let mut choice: HashMap<usize, (usize, usize)> = HashMap::new();
        let mut starts: Vec<(&usize, &(f64, usize))> = scores[index].iter().collect();
        starts.sort_by_key(|(j, _)| **j);
        for (&start, &(score, _)) in starts {
            let Some((so_far, from)) = prefix[start] else { continue };
            let after = (start + span).min(count);
            let total = (so_far.0 + credit(score), so_far.1 + start);
            if next[after].is_none_or(|kept| better(total, kept)) {
                next[after] = Some(total);
                choice.insert(after, (from, start));
            }
        }
        best = next;
        back.push(choice);
    }
    let mut position = (0..=count).fold(0, |kept, candidate| match (best[candidate], best[kept]) {
        (Some(offered), Some(held)) if better(offered, held) => candidate,
        (Some(_), None) => candidate,
        _ => kept,
    });
    let mut starts: Vec<Option<usize>> = vec![None; cues.len()];
    for index in (0..cues.len()).rev() {
        if let Some(&(from, start)) = back[index].get(&position) {
            starts[index] = Some(start);
            position = from;
        }
    }

    // A clearly recognised line was credited in full, so its start may sit a
    // word early; settle it where it matches best, between its neighbours.
    for index in 0..cues.len() {
        let Some(start) = starts[index] else { continue };
        let floor = starts[..index].iter().rev().flatten().next().map(|previous| previous + 1).unwrap_or(0);
        let ceiling = starts[index + 1..].iter().flatten().next().copied().unwrap_or(count);
        let low = start.saturating_sub(2).max(floor);
        let high = (start + 2).min(ceiling.saturating_sub(1)).min(count.saturating_sub(1));
        let score_at = |j: usize| scores[index].get(&j).map(|v| v.0).unwrap_or(0.0);
        let settled = (low..=high).fold(start, |kept, candidate| if score_at(candidate) > score_at(kept) { candidate } else { kept });
        starts[index] = Some(settled);
    }

    // Тайминги: совпавшие — по словам; конец не дальше начала следующей совпавшей реплики.
    let mut placed: Vec<Option<Placed>> = starts
        .iter()
        .enumerate()
        .map(|(index, start)| {
            start.map(|j| {
                let len = scores[index].get(&j).map(|v| v.1).unwrap_or(1).max(1);
                let last = (j + len - 1).min(count - 1);
                Placed { start: heard[j].1, end: heard[last].2, how: How::Aligned }
            })
        })
        .collect();
    let matched = placed.iter().filter(|p| p.is_some()).count();
    // Сдвиг файла — медиана (новое начало − начало в файле) по совпавшим репликам.
    let mut aligned_deltas: Vec<f64> = placed
        .iter()
        .zip(cues)
        .filter_map(|(p, (s, _, _))| p.as_ref().map(|p| p.start - s))
        .collect();
    aligned_deltas.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let offset = aligned_deltas.get(aligned_deltas.len() / 2).copied().unwrap_or(window_offset);

    // Несопоставленные: смещение (новое начало − начало в файле) ближайших совпавших соседей,
    // интерполяция по времени; длительность реплики из файла сохраняется.
    let deltas: Vec<Option<f64>> = placed
        .iter()
        .zip(cues)
        .map(|(p, (s, _, _))| p.as_ref().map(|p| p.start - s))
        .collect();
    for index in 0..cues.len() {
        if placed[index].is_some() {
            continue;
        }
        let (cs, ce, _) = cues[index];
        let prev = (0..index).rev().find_map(|k| deltas[k].map(|d| (cues[k].0, d)));
        let next = (index + 1..cues.len()).find_map(|k| deltas[k].map(|d| (cues[k].0, d)));
        let delta = match (prev, next) {
            (Some((pt, pd)), Some((nt, nd))) if nt > pt => pd + (nd - pd) * ((cs - pt) / (nt - pt)).clamp(0.0, 1.0),
            (Some((_, d)), _) | (None, Some((_, d))) => d,
            (None, None) => offset,
        };
        placed[index] = Some(Placed { start: cs + delta, end: ce + delta, how: How::Shifted });
    }

    // Порядок и неперекрытие: реплика кончается не позже начала следующей, не короче MIN_DUR.
    let mut out: Vec<Placed> = placed.into_iter().map(|p| p.expect("каждая реплика размещена выше")).collect();
    for index in 0..out.len() {
        out[index].start = out[index].start.max(0.0);
        if index + 1 < out.len() {
            let next_start = out[index + 1].start;
            if out[index].end > next_start {
                out[index].end = next_start.max(out[index].start + MIN_DUR);
            }
        }
        if out[index].end < out[index].start + MIN_DUR {
            out[index].end = out[index].start + MIN_DUR;
        }
    }
    Some(Alignment { cues: out, offset, share: matched as f64 / cues.len() as f64 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speak(from: f64, text: &str) -> Vec<Heard> {
        text.split_whitespace()
            .enumerate()
            .map(|(i, w)| (w.to_string(), from + i as f64 * 0.4, from + i as f64 * 0.4 + 0.3))
            .collect()
    }

    /// The characters two strings share in order, over both their lengths: 1 for
    /// the same text, lower for what either one misses or adds.
    fn dice(expected: &str, heard: &str) -> f64 {
        let (left, right) = (expected.chars().count(), heard.chars().count());
        if left + right == 0 {
            return 0.0;
        }
        2.0 * similarity(expected, heard) * left as f64 / (left + right) as f64
    }

    /// How much of the written line the recogniser heard, compared character by
    /// character rather than word by word.
    fn similarity(expected: &str, heard: &str) -> f64 {
        if expected.is_empty() || heard.is_empty() {
            return 0.0;
        }
        let left: Vec<char> = expected.chars().collect();
        let right: Vec<char> = heard.chars().collect();
        let mut previous = vec![0usize; right.len() + 1];
        let mut current = vec![0usize; right.len() + 1];
        for &lc in &left {
            for r in 0..right.len() {
                current[r + 1] = if lc == right[r] { previous[r] + 1 } else { current[r].max(previous[r + 1]) };
            }
            std::mem::swap(&mut previous, &mut current);
            current.iter_mut().for_each(|value| *value = 0);
        }
        previous[right.len()] as f64 / left.len() as f64
    }

    #[test]
    fn best_window_equals_the_studio_dice_over_each_window() {
        let heard: Vec<(String, f64, f64)> = ["neon", "arms", "the", "grass", "tonight", "again"]
            .iter()
            .enumerate()
            .map(|(i, w)| (w.to_string(), i as f64, i as f64 + 0.5))
            .collect();
        for line in ["Neon on the glass", "the grass tonight", "again"] {
            let toks: Vec<String> = line.split_whitespace().map(normalise).collect();
            let written: String = toks.concat();
            let chars: Vec<char> = written.chars().collect();
            for start in 0..heard.len() {
                let span = toks.len();
                let (got, len) = best_window(&chars, &heard, start, span);
                let want = (span.saturating_sub(1).max(1)..=span + 2)
                    .filter(|l| start + l <= heard.len())
                    .map(|l| dice(&written, &heard[start..start + l].iter().map(|h| h.0.as_str()).collect::<String>()))
                    .fold(0.0, f64::max);
                assert!((got - want).abs() < 1e-12, "{line} @{start}: {got} vs {want} (len {len})");
            }
        }
        assert_eq!(similarity("", "x"), 0.0);
    }

    #[test]
    fn a_shifted_release_snaps_to_the_speech() {
        // Субтитры другого релиза: всё на 3.2 с раньше речи.
        let mut words = speak(10.0, "where are you going tonight");
        words.extend(speak(14.0, "to the old harbour with my brother"));
        words.extend(speak(20.0, "come back before the storm"));
        let cues = [(6.8, 9.0, "Where are you going tonight?"), (10.8, 13.5, "To the old harbour, with my brother."), (16.8, 19.0, "Come back before the storm!")];
        let a = align(&cues, &words).unwrap();
        assert!((a.offset - 3.2).abs() < 0.3, "сдвиг файла {}", a.offset);
        assert_eq!(a.share, 1.0);
        assert_eq!(a.cues[0], Placed { start: 10.0, end: 11.9, how: How::Aligned });
        assert_eq!(a.cues[1].start, 14.0);
        assert!((a.cues[1].end - 16.7).abs() < 1e-9, "{:?}", a.cues[1]);
        assert_eq!(a.cues[2].start, 20.0);
    }

    #[test]
    fn a_release_off_by_more_than_the_window_is_found_by_rare_words() {
        let lines = [
            "the lighthouse keeper lost his lantern",
            "nobody believed the fisherman",
            "storms come from the western ridge",
            "bring the ropes before midnight",
        ];
        let mut words = Vec::new();
        let mut cues_owned = Vec::new();
        for (i, l) in lines.iter().enumerate() {
            let t = 10.0 + i as f64 * 6.0;
            words.extend(speak(t + 45.0, l));
            cues_owned.push((t, t + 3.0, *l));
        }
        let a = align(&cues_owned, &words).unwrap();
        assert_eq!(a.share, 1.0);
        assert!((a.offset - 45.0).abs() < 1e-9, "{}", a.offset);
    }

    #[test]
    fn misheard_words_still_place_the_line() {
        let words = speak(5.0, "neon arms the grass");
        let a = align(&[(4.0, 6.0, "Neon on the glass")], &words).unwrap();
        assert_eq!(a.cues[0].start, 5.0);
        assert_eq!(a.cues[0].how, How::Aligned);
    }

    #[test]
    fn a_repeated_line_is_consumed_in_order() {
        let mut words = speak(1.0, "yes");
        words.extend(speak(3.0, "i said no"));
        words.extend(speak(6.0, "yes"));
        let cues = [(0.5, 1.5, "Yes!"), (2.5, 4.5, "I said no."), (5.5, 6.5, "Yes!")];
        let a = align(&cues, &words).unwrap();
        assert_eq!(a.cues[0].start, 1.0);
        assert_eq!(a.cues[2].start, 6.0);
    }

    #[test]
    fn an_unheard_line_moves_with_its_neighbours_and_keeps_its_length() {
        let mut words = speak(12.0, "first line here");
        words.extend(speak(22.0, "third line here"));
        // Средняя реплика — пение, её слов нет в распознанном.
        let cues = [(10.0, 11.5, "First line here"), (15.0, 17.0, "la la la la"), (20.0, 21.5, "Third line here")];
        let a = align(&cues, &words).unwrap();
        assert_eq!(a.cues[1].how, How::Shifted);
        assert!((a.cues[1].start - 17.0).abs() < 1e-9, "{:?}", a.cues[1]);
        assert!((a.cues[1].end - a.cues[1].start - 2.0).abs() < 1e-9);
    }

    #[test]
    fn subtitles_in_another_language_barely_match() {
        let words = speak(1.0, "hello my friend how are you today");
        let cues = [(1.0, 2.0, "Привет, мой друг"), (2.0, 3.5, "Как ты сегодня?")];
        let a = align(&cues, &words).unwrap();
        assert!(a.share < MIN_ALIGNED_SHARE, "{}", a.share);
    }

    #[test]
    fn a_long_episode_aligns_fast_enough() {
        // 400 реплик × ~10 слов = 4000 слов; дрейф 1.5 с на весь эпизод.
        let vocab = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india", "juliet", "kilo", "lima", "mike"];
        let mut words = Vec::new();
        let mut cues_owned: Vec<(f64, f64, String)> = Vec::new();
        for i in 0..400usize {
            let text: Vec<&str> = (0..10).map(|k| vocab[(i * 7 + k * 3 + i / 13) % vocab.len()]).collect();
            let t = i as f64 * 5.0;
            words.extend(speak(t + 2.0 + 1.5 * i as f64 / 400.0, &text.join(" ")));
            cues_owned.push((t, t + 4.0, text.join(" ")));
        }
        let cues: Vec<(f64, f64, &str)> = cues_owned.iter().map(|(s, e, t)| (*s, *e, t.as_str())).collect();
        let started = std::time::Instant::now();
        let a = align(&cues, &words).unwrap();
        assert!(a.share > 0.9, "{}", a.share);
        assert!((a.cues[399].start - (399.0 * 5.0 + 2.0 + 1.5 * 399.0 / 400.0)).abs() < 0.05, "{:?}", a.cues[399]);
        assert!(started.elapsed().as_secs() < 60, "{:?}", started.elapsed());
    }
}
