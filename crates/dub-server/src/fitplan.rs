//! Прогноз укладки перевода в слот для редактора и агента: на каждую озвучиваемую реплику `fit` {оценка,
//! слот, отношение, вердикт, откалиброван ли темп} и, если последний рендер озвучил её текущий текст,
//! отчёт `fit.rendered` о том, как она на деле уложилась. Слот и кап — dub_core::fit, те же, что у рендера;
//! темп голоса — медиана его клипов из dub_timing.json, пока их меньше трёх — таблица языка.

use std::collections::HashMap;
use std::path::Path;

use dub_core::fit::{self, Calib, FitRules, Slot, Verdict};
use dub_core::{Project, Segment};
use serde_json::{json, Value};

use crate::dub_timing::DubTiming;

/// Поля реплики, которые сервер вычисляет для ответа и не хранит в project.json.
pub const COMPUTED: [&str; 2] = ["fit", "takes"];

fn flag(sel: &Value, key: &str, default_on: bool) -> bool {
    sel.get(key).and_then(Value::as_str).map(|v| v != "0").unwrap_or(default_on)
}

/// Настройки подгонки из active.json — те же, что читает рендер.
pub fn rules(models_root: &Path, max_stretch: f64) -> FitRules {
    let sel = crate::models::load_selection(models_root);
    FitRules { speech_rate_on: flag(&sel, "speech_rate_on", true), qc_duration: flag(&sel, "qc_duration", true), max_stretch }
}

/// «Сокращать перевод, если фраза не влезла» (включено по умолчанию).
pub fn auto_shorten_on(models_root: &Path) -> bool {
    flag(&crate::models::load_selection(models_root), "auto_shorten", true)
}

/// Реплика озвучивается: не скрыта, не оставлена оригиналом, текст есть.
pub fn voiced(s: &Segment) -> bool {
    let on = |k: &str| s.extra.get(k).and_then(Value::as_bool).unwrap_or(false);
    !on("hidden") && !on("keep_original") && !s.tgt_text.trim().is_empty()
}

pub fn speaker_of(s: &Segment) -> String {
    s.speaker.clone().unwrap_or_else(|| "0".to_string())
}

/// Темп голоса каждого спикера по клипам последнего рендера на языке проекта.
#[derive(Clone, Debug, Default)]
pub struct Calibration {
    lang: String,
    by_speaker: HashMap<String, Calib>,
}

impl Calibration {
    pub fn from_samples(lang: &str, samples: impl IntoIterator<Item = (String, usize, f64)>) -> Self {
        let mut grouped: HashMap<String, Vec<(usize, f64)>> = HashMap::new();
        for (spk, units, secs) in samples {
            grouped.entry(spk).or_default().push((units, secs));
        }
        let by_speaker = grouped.into_iter().filter_map(|(spk, v)| fit::calibrate(v).map(|c| (spk, c))).collect();
        Calibration { lang: lang.to_string(), by_speaker }
    }

    pub fn from_timing(timing: Option<&DubTiming>, lang: &str) -> Self {
        let samples = timing.into_iter().flat_map(|t| t.segments.values()).filter_map(|t| {
            let f = t.fit.as_ref()?;
            f.lang.eq_ignore_ascii_case(lang).then(|| (f.speaker.clone(), fit::text_units(&t.text), f.raw))
        });
        Self::from_samples(lang, samples)
    }

    /// Темп речи спикера и откалиброван ли он.
    pub fn cps(&self, speaker: &str) -> (f64, bool) {
        match self.by_speaker.get(speaker) {
            Some(c) => (c.cps, true),
            None => (fit::table_cps(&self.lang), false),
        }
    }
}

/// Слот реплики `i` без учёта отставания дубля (прогноз до рендера): укладка с её начала, до начала
/// следующей реплики или конца ролика.
pub fn predicted_slot(proj: &Project, i: usize, rules: &FitRules) -> Slot {
    let s = &proj.segments[i];
    let total = if proj.meta.duration > 0.0 { proj.meta.duration } else { s.end };
    let next = proj.segments.get(i + 1).map(|n| n.start).unwrap_or(total);
    fit::slot(s.start, s.end, s.start, next, rules)
}

fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Вердикт и отчёт укладки одной реплики.
pub struct SegFit {
    pub est: f64,
    pub slot: Slot,
    pub cps: f64,
    pub calibrated: bool,
    pub verdict: Verdict,
    pub rendered: Option<crate::dub_timing::FitRecord>,
    /// Длительность фразы в финальной дорожке (из отчёта рендера).
    pub laid: Option<f64>,
}

impl SegFit {
    /// Не влезает: по отчёту рендера этого текста, а без него — по прогнозу.
    pub fn over(&self) -> bool {
        match &self.rendered {
            Some(r) => fit::over(r.needed, r.cap),
            None => self.verdict == Verdict::Impossible,
        }
    }

    /// Слот, в который надо уложить сокращённый текст: фактический слот рендера, иначе прогнозный.
    pub fn target(&self) -> f64 {
        self.rendered.as_ref().map(|r| r.slot).unwrap_or(self.slot.target)
    }

    pub fn to_json(&self) -> Value {
        let rendered = self.rendered.as_ref().map(|r| {
            json!({
                "needed": r2(r.needed), "cap": r2(r.cap), "eff_cap": r2(r.eff_cap), "raw": r2(r.raw), "slot": r2(r.slot),
                "dur": self.laid.map(r2), "over": fit::over(r.needed, r.cap),
            })
        });
        json!({
            "est": r2(self.est), "slot": r2(self.slot.target), "ratio": r2(self.est / self.slot.target.max(1e-9)),
            "verdict": self.verdict, "calibrated": self.calibrated, "cps": (self.cps * 10.0).round() / 10.0,
            "cap": r2(self.slot.cap), "over": self.over(), "rendered": rendered,
        })
    }
}

/// Прогноз и отчёт укладки реплики `i`; None — реплика не озвучивается.
pub fn seg_fit(proj: &Project, i: usize, rules: &FitRules, calib: &Calibration, timing: Option<&DubTiming>) -> Option<SegFit> {
    let s = &proj.segments[i];
    if !voiced(s) {
        return None;
    }
    let slot = predicted_slot(proj, i, rules);
    let (cps, calibrated) = calib.cps(&speaker_of(s));
    let est = fit::estimate(&s.tgt_text, cps);
    let fresh = timing.and_then(|t| t.fresh(s));
    Some(SegFit {
        est,
        slot,
        cps,
        calibrated,
        verdict: fit::verdict(est, &slot),
        rendered: fresh.and_then(|t| t.fit.clone()),
        laid: fresh.map(|t| t.dur),
    })
}

/// Прогноз по всем репликам проекта. Укладка считается только у дубляжа и закадра.
pub fn project_fits(dir: &Path, proj: &Project, rules: &FitRules) -> Result<Vec<Option<SegFit>>, String> {
    if !matches!(proj.mode.as_str(), "dub" | "voiceover") {
        return Ok(proj.segments.iter().map(|_| None).collect());
    }
    let timing = DubTiming::load(dir)?;
    let calib = Calibration::from_timing(timing.as_ref(), &proj.tgt_lang);
    Ok((0..proj.segments.len()).map(|i| seg_fit(proj, i, rules, &calib, timing.as_ref())).collect())
}

/// Темп голоса каждой реплики для бюджета промпта перевода: калибровка последнего рендера или таблица.
pub fn cps_by_segment(dir: &Path, proj: &Project) -> Result<Vec<f64>, String> {
    let timing = DubTiming::load(dir)?;
    let calib = Calibration::from_timing(timing.as_ref(), &proj.tgt_lang);
    Ok(proj.segments.iter().map(|s| calib.cps(&speaker_of(s)).0).collect())
}

/// Project как JSON ответа: у реплик вычисленные `fit` (прогноз и отчёт укладки) и `takes` (сводка
/// истории дублей).
pub fn decorate(dir: &Path, proj: &Project, rules: &FitRules) -> Result<Value, String> {
    let fits = project_fits(dir, proj, rules)?;
    let takes = crate::takes::summaries(dir)?;
    let mut v = serde_json::to_value(proj).map_err(|e| e.to_string())?;
    if let Some(rows) = v.get_mut("segments").and_then(Value::as_array_mut) {
        for ((row, s), f) in rows.iter_mut().zip(&proj.segments).zip(&fits) {
            if let Some(f) = f {
                row["fit"] = f.to_json();
            }
            if let Some(t) = crate::render::seg_file_id(&s.id).and_then(|sid| takes.get(&sid)) {
                row["takes"] = t.clone();
            }
        }
    }
    Ok(v)
}

/// Убрать вычисленные поля, пришедшие с проектом от клиента (PUT отдаёт то, что получил в ответах).
pub fn strip_computed(proj: &mut Project) {
    for s in &mut proj.segments {
        for k in COMPUTED {
            s.extra.remove(k);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dub_timing::{FitRecord, SegTiming};

    const RULES: FitRules = FitRules { speech_rate_on: false, qc_duration: true, max_stretch: 1.25 };

    fn seg(id: &str, start: f64, end: f64, spk: &str, tgt: &str) -> Segment {
        Segment { id: id.into(), start, end, speaker: Some(spk.into()), tgt_text: tgt.into(), ..Default::default() }
    }

    fn timing_of(rows: &[(&Segment, f64, f64)]) -> DubTiming {
        let mut t = DubTiming::default();
        for (s, raw, slot) in rows {
            t.segments.insert(
                s.id.clone(),
                SegTiming {
                    text: s.tgt_text.trim().into(),
                    seg_start: s.start,
                    seg_end: s.end,
                    at: s.start,
                    dur: raw.min(*slot),
                    words: Vec::new(),
                    fit: Some(FitRecord {
                        raw: *raw, slot: *slot, needed: raw / slot, cap: 1.25, eff_cap: 1.25,
                        speaker: s.speaker.clone().unwrap(), voice: "clone".into(), lang: "ru".into(),
                    }),
                },
            );
        }
        t
    }

    #[test]
    fn prediction_uses_the_language_table_until_three_clips_calibrate_the_voice() {
        let mut p = Project { mode: "dub".into(), tgt_lang: "ru".into(), ..Default::default() };
        p.meta.duration = 20.0;
        p.segments = vec![
            seg("a", 0.0, 2.0, "0", &"я".repeat(26)),
            seg("b", 2.0, 4.0, "0", &"я".repeat(31)),
            seg("c", 4.0, 6.0, "0", &"я".repeat(40)),
            seg("d", 6.0, 8.0, "1", "   "),
        ];
        let cal = Calibration::from_timing(None, "ru");
        let f: Vec<_> = (0..4).map(|i| seg_fit(&p, i, &RULES, &cal, None)).collect();
        assert!(f[3].is_none(), "an empty line is not voiced");
        let a = f[0].as_ref().unwrap();
        assert!(!a.calibrated && (a.cps - 13.0).abs() < 1e-9 && (a.est - 2.0).abs() < 1e-9);
        assert_eq!(a.verdict, Verdict::Fits);
        assert_eq!(f[1].as_ref().unwrap().verdict, Verdict::Tight);
        assert_eq!(f[2].as_ref().unwrap().verdict, Verdict::Impossible);
        assert!(f[2].as_ref().unwrap().over() && !f[1].as_ref().unwrap().over());

        let slow = timing_of(&[(&p.segments[0], 2.6, 2.0), (&p.segments[1], 3.1, 2.0), (&p.segments[2], 4.0, 2.0)]);
        let cal = Calibration::from_timing(Some(&slow), "ru");
        let (cps, calibrated) = cal.cps("0");
        assert!(calibrated && (cps - 10.0).abs() < 1e-9, "{cps}");
        assert_eq!(cal.cps("1"), (13.0, false));
        assert!(!Calibration::from_timing(Some(&slow), "en").cps("0").1, "other language clips do not calibrate");
        let a = seg_fit(&p, 0, &RULES, &cal, Some(&slow)).unwrap();
        assert_eq!(a.verdict, Verdict::Impossible, "the calibrated voice reads 26 characters in 2.6 s");
        assert!(a.over(), "the render report of this text decides: 2.6 s in a 2 s slot is over the 1.25 cap");
        let j = a.to_json();
        assert_eq!(j["rendered"]["needed"], 1.3);
        assert_eq!(j["calibrated"], true);
        assert_eq!(j["verdict"], "impossible");
    }

    #[test]
    fn decorate_adds_computed_fields_and_strip_drops_them() {
        let d = std::env::temp_dir().join(format!("dub_fitplan_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let mut p = Project { mode: "dub".into(), tgt_lang: "en".into(), ..Default::default() };
        p.segments = vec![seg("s0", 0.0, 1.0, "0", "Hello there")];
        let v = decorate(&d, &p, &RULES).unwrap();
        assert!(v["segments"][0]["fit"]["verdict"].is_string());
        let mut back: Project = serde_json::from_value(v).unwrap();
        strip_computed(&mut back);
        assert!(back.segments[0].extra.is_empty());
        p.mode = "nodub".into();
        assert!(decorate(&d, &p, &RULES).unwrap()["segments"][0].get("fit").is_none());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
