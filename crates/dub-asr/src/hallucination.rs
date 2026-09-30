//! Фильтр галлюцинаций распознавания: фразы, которые ASR пишет поверх музыки и тишины вместо
//! «ничего не услышал» (титры субтитровщиков, «Продолжение следует», «Thanks for watching», звуки в
//! скобках). Список и правила — из студий (YuE2-Studio lyrics_sync.rs), адаптированы под дубляж:
//! сегмент судится целиком, поэтому реплика, лишь содержащая такую фразу, не срабатывает.

/// Фразы, которые Whisper пишет поверх музыки и тишины: дословные галлюцинации исследования
/// «Bag of Hallucinations» (Barański et al., ICASSP 2025, MIT) и поязычные списки конвейера
/// NVIDIA NeMo Granary (Apache-2.0) в сборке Scicom-AI/Whisper-Hallucination; фразы от двух слов,
/// нормализованные так же, как `normalised`. Файл идентичен копиям студий.
const HALLUCINATIONS: &str = include_str!("whisper_hallucinations.txt");

/// Слова, встречающиеся только в титрах субтитровщиков. Сравнение идёт по нормализованному тексту,
/// где точка и дефис — разделители, поэтому «amara org» и «sous titr» записаны через пробел.
const CREDIT_MARKERS: &[&str] = &[
    "dimatorzok",
    "субтитр",
    "amara org",
    "untertitel",
    "sous titr",
    "subtítulo",
    "sottotitol",
    "legendas por",
    "legendas pela",
    "subtitles by",
    "ondertitel",
    "napisy stworzone",
    "titulky vytvořil",
    "johnyx",
    "altyazı m k",
    "نانسي قنقر",
    "字幕",
    "자막",
];

/// Какой набор правил применять к сегменту.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HallucinationRules {
    /// Whisper: полный студийный набор, включая «КРИК» (капсом Whisper пишет звуки: «ВЕСЕЛАЯ МУЗЫКА»)
    /// и сегмент без букв (« 1.»).
    Whisper,
    /// Parakeet и облачный STT: регистр у них настоящий («NASA!» — реплика), числа пишутся цифрами,
    /// поэтому правило крика выключено, а «пусто» — это ни букв, ни цифр.
    CaseAware,
}

fn normalised(text: &str) -> String {
    let lowered = text.to_lowercase().replace('ё', "е");
    lowered
        .split(|c: char| c.is_whitespace() || ".,!?…\"'«»“”„-–—:;()[]♪。、！？「」".contains(c))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn known_phrases() -> &'static std::collections::HashSet<&'static str> {
    static KNOWN: std::sync::OnceLock<std::collections::HashSet<&'static str>> = std::sync::OnceLock::new();
    KNOWN.get_or_init(|| HALLUCINATIONS.lines().map(str::trim).filter(|line| !line.is_empty()).collect())
}

/// Является ли распознанный сегмент заполнением паузы, а не сказанными словами: известная галлюцинация
/// целиком, титр субтитровщика, звук, записанный как субтитр («[Music]», «♪»), крик капсом (только
/// Whisper) или текст без слов.
pub fn is_hallucination(text: &str, rules: HallucinationRules) -> bool {
    let trimmed = text.trim();
    let wordless = match rules {
        HallucinationRules::Whisper => !trimmed.chars().any(char::is_alphabetic),
        HallucinationRules::CaseAware => !trimmed.chars().any(char::is_alphanumeric),
    };
    if wordless {
        return true;
    }
    let bracketed = (trimmed.starts_with('[') && trimmed.ends_with(']'))
        || (trimmed.starts_with('(') && trimmed.ends_with(')'))
        || trimmed.starts_with('♪');
    if bracketed {
        return true;
    }
    if rules == HallucinationRules::Whisper {
        let letters: Vec<char> = trimmed.chars().filter(|c| c.is_alphabetic()).collect();
        let shouted = letters.len() >= 3
            && letters.iter().all(|c| !c.is_lowercase())
            && letters.iter().any(|c| c.is_uppercase());
        if shouted {
            return true;
        }
    }
    let plain = normalised(trimmed);
    if CREDIT_MARKERS.iter().any(|marker| plain.contains(marker)) {
        return true;
    }
    known_phrases().contains(plain.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: HallucinationRules = HallucinationRules::Whisper;
    const C: HallucinationRules = HallucinationRules::CaseAware;

    #[test]
    fn whisper_fillers_are_dropped_and_real_lines_kept() {
        assert!(is_hallucination(" Субтитры создавал DimaTorzok", W));
        assert!(is_hallucination("Продолжение следует...", W));
        assert!(is_hallucination("Thanks for watching!", W));
        assert!(is_hallucination("ご視聴ありがとうございました", W));
        assert!(is_hallucination("ВЕСЕЛАЯ МУЗЫКА", W));
        assert!(is_hallucination("[Music]", W));
        assert!(is_hallucination(" 1.", W));
        assert!(!is_hallucination("Если б мне платили каждый раз,", W));
        assert!(!is_hallucination("Спасибо, что ты рядом со мной", W));
        assert!(!is_hallucination("Поехали!", W));
    }

    #[test]
    fn credits_in_the_dub_languages_are_caught() {
        assert!(is_hallucination("Subtitles by the Amara.org community", C));
        assert!(is_hallucination("Sous-titres réalisés par la communauté d'Amara.org", C));
        assert!(is_hallucination("Napisy stworzone przez społeczność Amara.org", C));
        assert!(is_hallucination("Ondertitels ingediend door de Amara.org gemeenschap", C));
        assert!(is_hallucination("Titulky vytvořil JohnyX", C));
        assert!(is_hallucination("Altyazı M.K.", C));
        assert!(is_hallucination("ترجمة نانسي قنقر", C));
        assert!(is_hallucination("Untertitel im Auftrag des ZDF, 2021", C));
        assert!(is_hallucination("Sous-titrage ST' 501", C));
    }

    #[test]
    fn case_aware_rules_keep_shouting_and_numbers() {
        assert!(!is_hallucination("NASA!", C));
        assert!(!is_hallucination("ПОМОГИТЕ!", C));
        assert!(!is_hallucination("2024.", C));
        assert!(is_hallucination("...", C));
        assert!(is_hallucination("(laughs)", C));
    }

    #[test]
    fn a_line_that_only_contains_a_known_phrase_stays() {
        assert!(is_hallucination("Thank you for watching.", C));
        assert!(!is_hallucination("Thank you for watching my brother while I was away.", C));
    }
}
