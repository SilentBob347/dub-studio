//! Глоссарий в промпте перевода: правило (стабильная часть инструкции), записи пакета (только найденные в
//! его строках, ручные раньше собранных), авто-пары имён и term-lock ответа.

use dub_core::glossary::{hits, normalize, GlossaryEntry, GlossarySource};

/// Правило глоссария: идёт в неизменную часть инструкции, записи — к строкам пакета.
pub(crate) const RULE: &str = " Follow the GLOSSARY given with the lines: 'term → translation' — render the term with exactly \
that translation (inflect it as the grammar needs); 'keep: term' — leave the term untranslated, exactly as written.";

/// Блок глоссария для строк пакета; пусто — ни один термин в них не встречается.
pub(crate) fn block(entries: &[GlossaryEntry], texts: &[&str]) -> String {
    let joined = texts.join("\n");
    let lines: Vec<String> = hits(entries, &joined).into_iter().filter_map(GlossaryEntry::prompt_line).collect();
    if lines.is_empty() {
        String::new()
    } else {
        format!("=== GLOSSARY ===\n{}\n\n", lines.join("\n"))
    }
}

/// Глоссарий с авто-парами имён: пары для терминов, которых в нём нет, — записи source:auto после ручных.
pub(crate) fn with_auto(entries: &[GlossaryEntry], pairs: Vec<(String, String)>, lang: &str) -> Vec<GlossaryEntry> {
    let mut out = entries.to_vec();
    for (term, translation) in pairs {
        if out.iter().any(|e| normalize(&e.term) == normalize(&term)) {
            continue;
        }
        out.push(GlossaryEntry { term, translation, source: GlossarySource::Auto, lang: lang.to_string(), ..GlossaryEntry::default() });
    }
    out
}

/// term-lock: исходный термин, утёкший в перевод непереведённым, заменить на его перевод (последняя страховка
/// к промпту). Регистрозависимо, целыми словами.
pub(crate) fn term_lock(line: &str, entries: &[GlossaryEntry]) -> String {
    let mut out = line.to_string();
    for e in entries {
        if e.keep || e.translation.is_empty() || e.term == e.translation || !out.contains(e.term.as_str()) {
            continue;
        }
        out = replace_word(&out, &e.term, &e.translation);
    }
    out
}

/// Заменить целые вхождения `src` (границы — не буквенно-цифровой символ) на `dst`. Не трогает src внутри
/// более длинных слов (напр. "Sam" в "Samples"). Учитывает Unicode-алфавит для границ.
fn replace_word(hay: &str, src: &str, dst: &str) -> String {
    let word = |c: char| c.is_alphanumeric();
    let mut out = String::with_capacity(hay.len());
    let mut rest = hay;
    while let Some(pos) = rest.find(src) {
        let before_ok = rest[..pos].chars().next_back().is_none_or(|c| !word(c));
        let after = &rest[pos + src.len()..];
        let after_ok = after.chars().next().is_none_or(|c| !word(c));
        out.push_str(&rest[..pos]);
        out.push_str(if before_ok && after_ok { dst } else { src });
        rest = after;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(term: &str, translation: &str) -> GlossaryEntry {
        GlossaryEntry { term: term.into(), translation: translation.into(), ..GlossaryEntry::default() }
    }

    #[test]
    fn only_the_terms_of_the_batch_go_into_its_prompt() {
        let mut keep = entry("Nvidia", "");
        keep.keep = true;
        let g = vec![entry("Hogwarts", "Хогвартс"), keep, entry("Dumbledore", "Дамблдор"), entry("Ghost", "")];
        let b = block(&g, &["Back to Hogwarts", "an Nvidia card"]);
        assert_eq!(b, "=== GLOSSARY ===\nHogwarts → Хогвартс\nkeep: Nvidia\n\n");
        assert_eq!(block(&g, &["nothing here", "a ghost"]), "", "an entry without translation or keep is not for the prompt");
    }

    #[test]
    fn auto_pairs_do_not_override_the_glossary() {
        let g = with_auto(&[entry("Harry", "Гарри")], vec![("Harry".into(), "Хэрри".into()), ("Ron".into(), "Рон".into())], "ru");
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].translation, "Гарри");
        assert_eq!((g[1].source, g[1].lang.as_str()), (GlossarySource::Auto, "ru"));
    }

    #[test]
    fn replace_word_whole_words_only() {
        assert_eq!(replace_word("Sam went home", "Sam", "Сэм"), "Сэм went home");
        assert_eq!(replace_word("Samples of Sam", "Sam", "Сэм"), "Samples of Сэм");
        assert_eq!(replace_word("Sam and Sam", "Sam", "Сэм"), "Сэм and Сэм");
        assert_eq!(replace_word("nothing", "Sam", "Сэм"), "nothing");
    }

    #[test]
    fn term_lock_applies_glossary() {
        let g = vec![entry("Sam", "Сэм"), entry("Bob", "Боб")];
        assert_eq!(term_lock("Sam met Bob today", &g), "Сэм met Боб today");
        assert_eq!(term_lock("Sam here", &[entry("Sam", "Sam")]), "Sam here");
        let mut keep = entry("Sam", "Сэм");
        keep.keep = true;
        assert_eq!(term_lock("Sam here", &[keep]), "Sam here", "a kept term stays");
        assert_eq!(term_lock("plain", &[]), "plain");
    }
}
