//! Глоссарий через API: проекта (поле glossary в project.json) и профиля сериала (glossary.json профиля в
//! библиотеке кастингов), импорт и экспорт TSV, «Собрать из текста» (джоба), отпечаток глоссария для кэша
//! перевода и признак «перевод устарел».

use std::collections::HashMap;

use axum::extract::{Path as AxPath, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use dub_core::glossary::{for_translation, from_tsv, merge_tsv, merge_under, to_tsv, validate};
use dub_core::{GlossaryEntry, Project};
use serde_json::{json, Value};

use crate::{casting_library, jobs, save_project_atomic, AppState};

/// Отпечаток того, что перевод на `tgt` берёт из глоссария (for_translation: термин, перевод, keep в порядке
/// промпта); пусто — таких записей нет (старый проект без глоссария не «устаревает»).
pub fn fingerprint(entries: &[GlossaryEntry], tgt: &str) -> String {
    let used = for_translation(entries, tgt);
    if used.is_empty() {
        return String::new();
    }
    let rows: Vec<(&str, &str, bool)> =
        used.iter().map(|e| (e.term.as_str(), if e.keep { "" } else { e.translation.as_str() }, e.keep)).collect();
    let json = serde_json::to_string(&rows).expect("строки глоссария сериализуются в JSON");
    blake3::hash(json.as_bytes()).to_hex().to_string()
}

/// Глоссарий менялся после перевода: перевод сделан с другим.
pub fn stale(p: &Project) -> bool {
    crate::translate::wants_translate(p) && fingerprint(&p.glossary, &p.tgt_lang) != p.glossary_fp
}

fn project_json(p: &Project) -> Value {
    json!({ "entries": p.glossary, "tgt_lang": p.tgt_lang, "casting_ref": p.casting_ref, "stale": stale(p) })
}

fn tsv(entries: &[GlossaryEntry]) -> Response {
    ([(header::CONTENT_TYPE, "text/tab-separated-values; charset=utf-8")], to_tsv(entries)).into_response()
}

/// Ответ как JSON или TSV (?format=json|tsv).
fn wants_tsv(q: &HashMap<String, String>) -> Result<bool, String> {
    match q.get("format").map(String::as_str) {
        None | Some("json") => Ok(false),
        Some("tsv") => Ok(true),
        Some(other) => Err(format!("format «{other}»: json или tsv")),
    }
}

/// Тело PUT: весь список entries или текст tsv; merge — влить в имеющиеся (присланное главнее) вместо замены,
/// у имеющихся терминов TSV меняет только свои колонки; lang — язык записей TSV и записей без языка с
/// переводом или произношением.
#[derive(serde::Deserialize)]
pub struct Put {
    entries: Option<Vec<GlossaryEntry>>,
    tsv: Option<String>,
    #[serde(default)]
    merge: bool,
    lang: Option<String>,
}

fn incoming(body: Put, default_lang: &str, current: &[GlossaryEntry]) -> Result<Vec<GlossaryEntry>, String> {
    let lang = body.lang.as_deref().unwrap_or(default_lang).trim().to_string();
    let is_tsv = body.tsv.is_some();
    let new = match (body.entries, body.tsv) {
        (Some(entries), None) => entries,
        (None, Some(text)) => from_tsv(&text, &lang)?,
        (Some(_), Some(_)) => return Err("нужно что-то одно: entries или tsv".into()),
        (None, None) => return Err("нужно entries (список записей) или tsv".into()),
    };
    let new: Vec<GlossaryEntry> = validate(new)?
        .into_iter()
        .map(|mut e| {
            if e.lang.is_empty() && (!e.translation.is_empty() || !e.pronunciation.is_empty()) {
                e.lang = lang.clone();
            }
            e
        })
        .collect();
    let merged = match (body.merge, is_tsv) {
        (false, _) => new,
        (true, false) => merge_under(&new, current),
        (true, true) => merge_tsv(&new, current),
    };
    validate(merged)
}

/// GET /projects/{pid}/glossary[?format=tsv] -> {entries, tgt_lang, casting_ref, stale} или TSV.
pub async fn project_get(State(st): State<AppState>, AxPath(pid): AxPath<String>, Query(q): Query<HashMap<String, String>>) -> Response {
    let as_tsv = match wants_tsv(&q) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    match st.load_project(&pid) {
        Ok(p) if as_tsv => tsv(&p.glossary),
        Ok(p) => Json(project_json(&p)).into_response(),
        Err(r) => r,
    }
}

/// PUT /projects/{pid}/glossary {entries | tsv, merge?, lang?} -> как GET. Перевод не трогает: если он
/// сделан с другим глоссарием, ответ говорит stale — «Перевести заново» (retranslate) обновит его.
pub async fn project_put(State(st): State<AppState>, AxPath(pid): AxPath<String>, Json(body): Json<Put>) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let mut p = match st.load_project(&pid) {
        Ok(p) => p,
        Err(r) => return r,
    };
    match incoming(body, &p.tgt_lang, &p.glossary) {
        Ok(entries) => p.glossary = entries,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    }
    if let Err(e) = save_project_atomic(&dir, &p) {
        return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response();
    }
    Json(project_json(&p)).into_response()
}

/// GET /casting/library/{slug}/glossary[?format=tsv] -> {slug, entries} или TSV.
pub async fn series_get(State(st): State<AppState>, AxPath(slug): AxPath<String>, Query(q): Query<HashMap<String, String>>) -> Response {
    let as_tsv = match wants_tsv(&q) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    match casting_library::read_glossary(&st.repo_root, &slug) {
        None => (StatusCode::NOT_FOUND, "profile not found").into_response(),
        Some(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, format!("глоссарий сериала не читается: {e}")).into_response(),
        Some(Ok(entries)) if as_tsv => tsv(&entries),
        Some(Ok(entries)) => Json(json!({ "slug": slug, "entries": entries })).into_response(),
    }
}

/// PUT /casting/library/{slug}/glossary {entries | tsv, merge?, lang?} -> {slug, entries}. «Сохранить в
/// профиль сериала» — записи проекта с merge: true.
pub async fn series_put(State(st): State<AppState>, AxPath(slug): AxPath<String>, Json(body): Json<Put>) -> Response {
    let current = if body.merge {
        match casting_library::read_glossary(&st.repo_root, &slug) {
            None => return (StatusCode::NOT_FOUND, "profile not found").into_response(),
            Some(Err(e)) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("глоссарий сериала не читается: {e}")).into_response(),
            Some(Ok(entries)) => entries,
        }
    } else {
        Vec::new()
    };
    let entries = match incoming(body, "", &current) {
        Ok(e) => e,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    match casting_library::write_glossary(&st.repo_root, &slug, &entries) {
        Ok(()) => Json(json!({ "slug": slug, "entries": entries })).into_response(),
        Err(e) if casting_library::read_glossary(&st.repo_root, &slug).is_none() => (StatusCode::NOT_FOUND, e).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

/// POST /projects/{pid}/glossary/extract -> {job_id}: кандидаты из текста проекта (проход модели + повторяющиеся
/// имена); итог джобы {entries} с source:auto — в глоссарий их вносит человек (PUT).
pub async fn extract(State(st): State<AppState>, AxPath(pid): AxPath<String>) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let proj_path = dir.join("project.json");
    if !proj_path.is_file() {
        return (StatusCode::CONFLICT, "project not analyzed yet").into_response();
    }
    let llama_bin = st.llama_bin.clone();
    let (mt_model, _) = crate::models::resolve_mt(&st.models_root, &crate::models::load_selection(&st.models_root));
    let models_root = st.models_root.clone();
    let job: jobs::JobFn = Box::new(move |progress: jobs::ProgressFn| {
        let text = std::fs::read_to_string(&proj_path).map_err(|e| e.to_string())?;
        let p = Project::from_json(&text).map_err(|e| e.to_string())?;
        let hidden = |s: &dub_core::Segment| s.extra.get("hidden").and_then(Value::as_bool).unwrap_or(false);
        let texts: Vec<String> = p
            .segments
            .iter()
            .filter(|s| !hidden(s))
            .map(|s| if s.src_text.trim().is_empty() { s.tgt_text.trim() } else { s.src_text.trim() }.to_string())
            .filter(|t| !t.is_empty())
            .collect();
        if texts.is_empty() {
            return Err("в проекте нет текста: глоссарий собирается из распознанной речи — сначала анализ".into());
        }
        let say = |m: &str| progress(json!({ "type": "progress", "stage": "glossary", "msg": m }));
        say(&format!("глоссарий: {} строк текста", texts.len()));
        let prov = crate::llm_provider::open(
            &crate::llm_provider::LlmOpen {
                llama_bin: &llama_bin,
                mt_model: &mt_model,
                mmproj: std::path::Path::new(""),
                models_root: &models_root,
            },
            crate::llm_provider::LlmMode::Text,
        )
        .map_err(|e| format!("глоссарий: LLM недоступен — {e}"))?;
        let src_lang = p.meta.extra.get("src_lang").and_then(Value::as_str).unwrap_or("auto").to_string();
        let found = dub_translate::extract_glossary(prov.client(), &texts, &src_lang, &p.tgt_lang, &p.glossary, &mut |m: &str| say(m))
            .map_err(|e| format!("глоссарий: {e}"))?;
        drop(prov);
        say(&format!("глоссарий: предложено записей — {}", found.len()));
        Ok(json!({ "entries": found }))
    });
    match st.jobs.enqueue(jobs::JobMeta::new(jobs::JobKind::Glossary, Some(&pid)), job).await {
        Ok(job_id) => Json(json!({ "job_id": job_id })).into_response(),
        Err(e) => crate::enqueue_error(e),
    }
}

/// Глоссарий анализа: записи прежнего project.json и профиля сериала (casting_ref), записи проекта главнее.
/// Профиля нет — строка в журнал; его глоссарий не читается — ошибка анализа.
pub fn for_analyze(
    repo_root: &std::path::Path,
    prev: Option<&Project>,
    casting_ref: &str,
    progress: &crate::analyze::Progress,
) -> Result<Vec<GlossaryEntry>, String> {
    let own: Vec<GlossaryEntry> = prev.map(|p| p.glossary.clone()).unwrap_or_default();
    let slug = casting_ref.trim();
    if slug.is_empty() {
        return Ok(own);
    }
    match casting_library::read_glossary(repo_root, slug) {
        None => {
            progress(json!({ "stage": "translate", "msg": format!("профиль сериала «{slug}» не найден — его глоссарий не применён") }));
            Ok(own)
        }
        Some(Err(e)) => Err(format!("глоссарий сериала «{slug}» не читается: {e}")),
        Some(Ok(series)) => {
            let merged = merge_under(&own, &series);
            progress(json!({ "stage": "translate", "msg": format!(
                "глоссарий сериала «{slug}»: {} записей, добавлено в проект {}", series.len(), merged.len() - own.len()) }));
            Ok(merged)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(term: &str, translation: &str) -> GlossaryEntry {
        GlossaryEntry { term: term.into(), translation: translation.into(), ..GlossaryEntry::default() }
    }

    fn put(entries: Option<Vec<GlossaryEntry>>, tsv: Option<&str>, merge: bool) -> Put {
        Put { entries, tsv: tsv.map(str::to_string), merge, lang: None }
    }

    #[test]
    fn the_fingerprint_follows_what_the_translation_uses() {
        assert_eq!(fingerprint(&[], "ru"), "");
        let ru = GlossaryEntry { lang: "ru".into(), ..entry("Harry", "Гарри") };
        let a = fingerprint(std::slice::from_ref(&ru), "ru");
        assert_ne!(a, "");
        assert_ne!(a, fingerprint(&[GlossaryEntry { translation: "Гари".into(), ..ru.clone() }], "ru"));
        assert_eq!(
            fingerprint(std::slice::from_ref(&ru), "es"),
            fingerprint(&[GlossaryEntry { translation: "Хэрри".into(), ..ru.clone() }], "es"),
            "another language's translation does not matter"
        );
        assert_ne!(a, fingerprint(&[GlossaryEntry { keep: true, ..ru.clone() }], "ru"));
        let spoken = GlossaryEntry {
            pronunciation: "Гэрри".into(),
            note: "hero".into(),
            asr_fix: vec!["hairy".into()],
            source: dub_core::glossary::GlossarySource::Auto,
            ..ru.clone()
        };
        assert_eq!(a, fingerprint(std::slice::from_ref(&spoken), "ru"), "pronunciation, note, ASR variants and source do not");
        let asr_only = GlossaryEntry { asr_fix: vec!["hogworts".into()], lang: String::new(), ..entry("Hogwarts", "") };
        let with_asr_only = [ru.clone(), asr_only];
        assert_eq!(a, fingerprint(&with_asr_only, "ru"), "an entry that sets no translation does not either");
        assert_eq!(fingerprint(&with_asr_only[1..], "ru"), "");
        let auto = GlossaryEntry { source: dub_core::glossary::GlossarySource::Auto, lang: "ru".into(), ..entry("Ron", "Рон") };
        assert_eq!(
            fingerprint(&[auto.clone(), ru.clone()], "ru"),
            fingerprint(&[ru.clone(), auto], "ru"),
            "the order is the prompt's: manual first"
        );
    }

    #[test]
    fn a_new_pronunciation_does_not_make_the_translation_stale() {
        let mut p = Project { tgt_lang: "ru".into(), mode: "dub".into(), glossary: vec![entry("Harry", "Гарри")], ..Project::default() };
        p.glossary_fp = fingerprint(&p.glossary, "ru");
        p.glossary[0].pronunciation = "Гэрри".into();
        p.glossary[0].note = "hero".into();
        assert!(!stale(&p));
        p.glossary.push(GlossaryEntry { asr_fix: vec!["hogworts".into()], ..entry("Hogwarts", "") });
        assert!(!stale(&p), "«Add to glossary» from the editor only fixes recognition");
        p.glossary[1].translation = "Хогвартс".into();
        assert!(stale(&p));
    }

    #[test]
    fn a_changed_glossary_makes_the_translation_stale() {
        let mut p = Project { tgt_lang: "ru".into(), mode: "dub".into(), ..Project::default() };
        assert!(!stale(&p), "no glossary, no translation made with one");
        p.glossary = vec![entry("Harry", "Гарри")];
        assert!(stale(&p));
        p.glossary_fp = fingerprint(&p.glossary, "ru");
        assert!(!stale(&p));
        p.mode = "transcribe".into();
        p.glossary.push(entry("Ron", "Рон"));
        assert!(!stale(&p), "a transcript has no translation");
    }

    #[test]
    fn analysis_takes_the_series_glossary_under_the_projects_own() {
        let repo = std::env::temp_dir().join(format!("dub_gloss_analyze_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        let seen = std::sync::Mutex::new(Vec::<String>::new());
        let progress = |v: Value| seen.lock().unwrap().push(v["msg"].as_str().unwrap_or_default().to_string());
        let prev = Project { glossary: vec![entry("Harry", "Гарри")], ..Project::default() };
        assert_eq!(for_analyze(&repo, Some(&prev), "", &progress).unwrap().len(), 1);
        assert_eq!(for_analyze(&repo, Some(&prev), "show", &progress).unwrap().len(), 1, "no profile: the project's own");
        assert!(seen.lock().unwrap()[0].contains("не найден"));
        let dir = casting_library::profile_dir(&repo, "show");
        std::fs::create_dir_all(&dir).unwrap();
        dub_faces::save_casting(&dir.join("casting.json"), &dub_faces::Casting::default()).unwrap();
        casting_library::write_glossary(&repo, "show", &[entry("harry", "Хэрри"), entry("Ron", "Рон")]).unwrap();
        let merged = for_analyze(&repo, Some(&prev), "show", &progress).unwrap();
        assert_eq!(merged.iter().map(|e| e.translation.as_str()).collect::<Vec<_>>(), vec!["Гарри", "Рон"]);
        std::fs::write(dir.join("glossary.json"), "{").unwrap();
        assert!(for_analyze(&repo, None, "show", &progress).unwrap_err().contains("не читается"));
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn put_replaces_or_merges_and_fills_the_language() {
        let current = vec![entry("Harry", "Гарри"), entry("Ron", "Рон")];
        let replaced = incoming(put(Some(vec![entry("Harry", "Хэрри")]), None, false), "ru", &current).unwrap();
        assert_eq!(replaced.len(), 1);
        assert_eq!(replaced[0].lang, "ru");
        let merged = incoming(put(Some(vec![entry("Harry", "Хэрри")]), None, true), "ru", &current).unwrap();
        assert_eq!(merged.iter().map(|e| e.translation.as_str()).collect::<Vec<_>>(), vec!["Хэрри", "Рон"]);
        let from_tsv = incoming(put(None, Some("term\ttranslation\tkeep\tpronunciation\nNvidia\t\t1\tЭнвидиа\n"), true), "ru", &current).unwrap();
        assert_eq!(from_tsv.len(), 3);
        assert!(from_tsv[2].keep && from_tsv[2].lang == "ru");
        assert!(incoming(put(None, None, false), "ru", &current).is_err());
        assert!(incoming(put(Some(vec![entry(" ", "x")]), None, false), "ru", &current).unwrap_err().contains("пустой"));
    }

    #[test]
    fn export_then_import_keeps_asr_variants_and_notes() {
        let mut harry = GlossaryEntry { lang: "ru".into(), note: "hero".into(), asr_fix: vec!["hairy".into()], ..entry("Harry", "Гарри") };
        harry.source = dub_core::glossary::GlossarySource::Auto;
        let current = vec![harry.clone(), GlossaryEntry { lang: "ru".into(), note: "friend".into(), ..entry("Ron", "Рон") }];
        let edited = to_tsv(&current).replace("Гарри", "Гарри Поттер");
        let back = incoming(put(None, Some(&edited), true), "ru", &current).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0], GlossaryEntry { translation: "Гарри Поттер".into(), ..harry });
        assert_eq!(back[1], current[1]);
    }
}
