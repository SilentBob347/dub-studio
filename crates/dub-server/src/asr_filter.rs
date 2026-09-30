//! Галлюцинации ASR в транскрипте дубляжа. Текстовое правило (dub_asr::is_hallucination) находит
//! кандидатов, а «скрыть или только пометить» решает голос на интервале реплики: «Thanks for watching»
//! в конце ролика часто сказано на самом деле. Реплику не удаляем — `extra.hidden` исключает её из
//! озвучки и субтитров, пользователь возвращает её в редакторе; `extra.asr_flag` объясняет почему.

use dub_asr::{is_hallucination, HallucinationRules};
use dub_core::Segment;
use serde_json::Value;
use std::path::Path;

/// Доля интервала реплики, где звучит голос, начиная с которой кандидат считается сказанным.
const VOICE_MIN_COVER: f64 = 0.3;

/// Чем подтверждается голос на интервале реплики.
pub enum VoiceEvidence {
    /// Речевые спаны огибающей ОТДЕЛЁННОГО вокала (BSRoformer): музыки в нём нет, энергия = голос.
    Vocals(Vec<(f64, f64)>),
    /// Реплики диаризации Sortformer (нейросетевая речевая активность).
    Turns(Vec<(f64, f64)>),
    /// Ни вокала, ни реплик: на сыром миксе энергия музыки неотличима от голоса, решает текст.
    TextOnly,
}

impl VoiceEvidence {
    /// Собрать свидетельство для analyze. `clean_vocals` — отделённый вокал 16 кГц, если сепарация
    /// сработала; `turns` — реплики диаризации (пусто при одном спикере или без диаризации).
    pub fn build(clean_vocals: Option<&Path>, turns: &[dub_asr::Turn]) -> Result<Self, String> {
        if let Some(p) = clean_vocals {
            let (samples, sr) = crate::wavio::read_mono_f32(p)?;
            let cfg = dub_asr::WindowConfig::default();
            let (env, frame_sec) = dub_asr::speech_envelope(&samples, sr, cfg.frame_sec);
            return Ok(VoiceEvidence::Vocals(dub_asr::detect_active_spans(&env, frame_sec, &cfg)));
        }
        if !turns.is_empty() {
            return Ok(VoiceEvidence::Turns(turns.iter().map(|t| (t.start, t.end)).collect()));
        }
        Ok(VoiceEvidence::TextOnly)
    }

    fn name(&self) -> &'static str {
        match self {
            VoiceEvidence::Vocals(_) => "vocals",
            VoiceEvidence::Turns(_) => "diarization",
            VoiceEvidence::TextOnly => "text",
        }
    }

    /// Есть ли голос на [start, end]. None — свидетельства нет.
    fn voiced(&self, start: f64, end: f64) -> Option<bool> {
        let spans = match self {
            VoiceEvidence::Vocals(s) | VoiceEvidence::Turns(s) => s,
            VoiceEvidence::TextOnly => return None,
        };
        let dur = (end - start).max(1e-3);
        let covered: f64 = spans.iter().map(|&(a, b)| (b.min(end) - a.max(start)).max(0.0)).sum();
        Some(covered / dur >= VOICE_MIN_COVER)
    }
}

/// Итог фильтра для журнала analyze.
#[derive(Debug, Default)]
pub struct Report {
    /// Скрытые реплики (голоса на интервале нет).
    pub hidden: Vec<String>,
    /// Похожи на галлюцинацию, но голос есть — оставлены, только помечены.
    pub voiced: Vec<String>,
}

/// Пометить реплики-галлюцинации. Помеченная реплика получает `extra.asr_flag = "hallucination"` и
/// `extra.asr_evidence` (vocals | diarization | text); без голоса — ещё и `extra.hidden = true`.
pub fn apply(segs: &mut [Segment], rules: HallucinationRules, evidence: &VoiceEvidence) -> Report {
    let mut report = Report::default();
    for s in segs.iter_mut() {
        if !is_hallucination(&s.src_text, rules) {
            continue;
        }
        let voiced = evidence.voiced(s.start, s.end).unwrap_or(false);
        s.extra.insert("asr_flag".into(), Value::String("hallucination".into()));
        s.extra.insert("asr_evidence".into(), Value::String(evidence.name().into()));
        let text = s.src_text.trim().to_string();
        if voiced {
            report.voiced.push(text);
        } else {
            s.extra.insert("hidden".into(), Value::Bool(true));
            report.hidden.push(text);
        }
    }
    report
}

/// Скрыта ли реплика (hidden).
pub fn is_hidden(s: &Segment) -> bool {
    s.extra.get("hidden").and_then(|v| v.as_bool()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, end: f64, text: &str) -> Segment {
        Segment { id: "s".into(), start, end, src_text: text.into(), ..Default::default() }
    }

    #[test]
    fn a_credit_over_silence_is_hidden() {
        let mut segs = vec![seg(0.0, 2.0, "Мы уходим завтра."), seg(10.0, 12.0, "Субтитры сделал DimaTorzok")];
        let ev = VoiceEvidence::Vocals(vec![(0.0, 2.1)]);
        let r = apply(&mut segs, HallucinationRules::Whisper, &ev);
        assert_eq!(r.hidden, vec!["Субтитры сделал DimaTorzok".to_string()]);
        assert!(!is_hidden(&segs[0]));
        assert!(is_hidden(&segs[1]));
        assert_eq!(segs[1].extra["asr_flag"], "hallucination");
        assert_eq!(segs[1].extra["asr_evidence"], "vocals");
    }

    #[test]
    fn a_real_thank_you_for_watching_with_voice_stays() {
        let mut segs = vec![seg(30.0, 31.5, "Спасибо за просмотр!")];
        let ev = VoiceEvidence::Vocals(vec![(29.9, 31.6)]);
        let r = apply(&mut segs, HallucinationRules::CaseAware, &ev);
        assert_eq!(r.voiced.len(), 1, "голос есть — реплика остаётся");
        assert!(!is_hidden(&segs[0]));
        assert_eq!(segs[0].extra["asr_flag"], "hallucination");
    }

    #[test]
    fn diarization_turns_count_as_voice() {
        let mut segs = vec![seg(5.0, 6.0, "Thanks for watching!")];
        let ev = VoiceEvidence::Turns(vec![(4.8, 6.2)]);
        apply(&mut segs, HallucinationRules::CaseAware, &ev);
        assert!(!is_hidden(&segs[0]));
        assert_eq!(segs[0].extra["asr_evidence"], "diarization");
    }

    #[test]
    fn without_evidence_the_text_decides() {
        let mut segs = vec![seg(5.0, 6.0, "Thanks for watching!")];
        apply(&mut segs, HallucinationRules::CaseAware, &VoiceEvidence::TextOnly);
        assert!(is_hidden(&segs[0]));
        assert_eq!(segs[0].extra["asr_evidence"], "text");
    }

    #[test]
    fn parakeet_shouting_is_speech() {
        let mut segs = vec![seg(1.0, 2.0, "NASA!")];
        let r = apply(&mut segs, HallucinationRules::CaseAware, &VoiceEvidence::TextOnly);
        assert!(r.hidden.is_empty() && r.voiced.is_empty());
        assert!(!segs[0].extra.contains_key("asr_flag"));
    }
}
