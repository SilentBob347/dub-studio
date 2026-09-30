//! A project's files for an agent: where they are on this computer, and its
//! lines written as SRT or plain text the way the window's export buttons
//! write them - or as WebVTT, JSON with word timings, or ASS styled as the
//! render burns them - into any folder and without opening Explorer.

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use dub_core::Project;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use crate::AppState;

/// GET /projects/{pid}/files — the project's folder and the files in it that
/// matter to the user, each null while it is not made yet.
pub async fn files(State(st): State<AppState>, AxPath(pid): AxPath<String>) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let existing = |path: PathBuf| if path.is_file() { Value::from(path.to_string_lossy().into_owned()) } else { Value::Null };
    let source = std::fs::read_to_string(dir.join("source.txt")).map(|s| PathBuf::from(s.trim())).map(existing).unwrap_or(Value::Null);
    let name = std::fs::read_to_string(dir.join("name.txt")).map(|s| Value::from(s.trim().to_string())).unwrap_or(Value::Null);
    let mut texts: Vec<String> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_file())
                .filter(|path| {
                    let file = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
                    (matches!(ext.as_str(), "srt" | "vtt" | "txt" | "ass") && file != "source.txt" && file != "name.txt")
                        || FIXED_NAMES.iter().any(|(format, _, fixed)| *format == "json" && *fixed == file)
                })
                .map(|path| path.to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    texts.sort();
    Json(json!({
        "folder": dir.to_string_lossy(),
        "name": name,
        "source": source,
        "project": existing(dir.join("project.json")),
        "output": existing(crate::find_output_save(&dir)),
        "playable_output": existing(crate::find_output(&dir)),
        "dub_audio": existing(dir.join("dub_audio.m4a")),
        "casting": existing(dir.join("casting.json")),
        "vocals": existing(dir.join("stems").join("vocals.wav")),
        "background": existing(dir.join("stems").join("instrumental.wav")),
        "texts": texts,
    }))
    .into_response()
}

/// The formats a project's lines are written in.
const FORMATS: &[&str] = &["srt", "vtt", "ass", "txt", "json"];

/// POST /projects/{pid}/export-text {format: srt|vtt|ass|txt|json, text?: tgt|src, dir?, name?,
/// speaker_label?, content?} — write the lines as a file. Without dir it goes
/// into the project's folder under the fixed name of its kind, replacing the
/// earlier one, as the window's save-text does; a name of one's own needs dir,
/// and there a name already taken gets (2), (3) instead of being overwritten.
/// content true answers the file's text as well.
pub async fn export_text(State(st): State<AppState>, AxPath(pid): AxPath<String>, Json(body): Json<Value>) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let proj = match st.load_project(&pid) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let format = body.get("format").and_then(Value::as_str).unwrap_or_default();
    if !FORMATS.contains(&format) {
        return (StatusCode::BAD_REQUEST, format!("format is one of {}, not {format:?}", FORMATS.join(", "))).into_response();
    }
    let which = body.get("text").and_then(Value::as_str).unwrap_or("tgt");
    let source = match which {
        "tgt" => false,
        "src" => true,
        other => return (StatusCode::BAD_REQUEST, format!("text is tgt or src, not {other:?}")).into_response(),
    };
    if format == "ass" && (proj.meta.width <= 0 || proj.meta.height <= 0) {
        return (StatusCode::BAD_REQUEST, "ass places the subtitles on the picture, and this project is audio: take srt, vtt, txt or json").into_response();
    }
    let label = body.get("speaker_label").and_then(Value::as_str).map(str::trim).filter(|l| !l.is_empty()).unwrap_or("Speaker");
    let rows = lines(&proj, source);
    let target = match destination(
        &dir,
        body.get("dir").and_then(Value::as_str),
        body.get("name").and_then(Value::as_str),
        format,
        source,
    ) {
        Ok(target) => target,
        Err(why) => return (StatusCode::BAD_REQUEST, why).into_response(),
    };
    let written = if format == "ass" {
        let fonts = st.fonts_dir.clone();
        let target = target.clone();
        tokio::task::spawn_blocking(move || write_ass(&proj, source, &target, &fonts, &dir))
            .await
            .unwrap_or_else(|e| Err(format!("ass: {e}")))
    } else {
        let content = render_text(&rows, format, label);
        std::fs::write(&target, &content).map(|_| content).map_err(|e| format!("write {}: {e}", target.display()))
    };
    let content = match written {
        Ok(content) => content,
        Err(why) => return (StatusCode::INTERNAL_SERVER_ERROR, why).into_response(),
    };
    let mut answer = json!({ "ok": true, "path": target.to_string_lossy(), "lines": rows.len() });
    if body.get("content").and_then(Value::as_bool) == Some(true) {
        answer["content"] = content.into();
    }
    Json(answer).into_response()
}

/// The lines as the text of a file of the format: srt, vtt, txt or json.
pub(crate) fn render_text(rows: &[Row], format: &str, label: &str) -> String {
    match format {
        "srt" => srt(rows),
        "vtt" => vtt(rows, label),
        "json" => format!("{:#}", json_rows(rows)),
        _ => txt(rows, label),
    }
}

/// The subtitles as ASS, styled as the render burns them (titles, per-line
/// text, hidden lines left out), even while they are switched off for the
/// render, and timed as it times them: in a dub each line where the dubbed
/// phrase sounds (the project's dub_timing.json). The transcript gets the
/// recognised words in the same style at the original speech's timing,
/// without the titles, which are the picture's translation.
fn write_ass(proj: &Project, source: bool, target: &Path, fonts: &Path, project_dir: &Path) -> Result<String, String> {
    let mut proj = proj.clone();
    proj.subs.mode = if source { "transcribe" } else { "translate" }.to_string();
    if source {
        for seg in &mut proj.segments {
            seg.tgt_text = seg.src_text.clone();
            seg.extra.remove("keep_original");
        }
        proj.mode = "nodub".to_string();
        proj.captions.overrides.clear();
        proj.captions.titles.clear();
    }
    dub_captions::set_fonts_dir(fonts);
    let timing = (!source).then_some(project_dir);
    crate::render::build_ass(&proj, target, timing, proj.meta.width, proj.meta.height, proj.meta.duration)?;
    std::fs::read_to_string(target).map_err(|e| format!("read {}: {e}", target.display()))
}

/// The name each format's lines take in the project's folder, by whether they
/// are the transcript. None is a name the studio keeps there for itself: the
/// JSON lines are not transcript.json, analyze's saved speech recognition.
const FIXED_NAMES: &[(&str, bool, &str)] = &[
    ("srt", false, "subtitles.srt"),
    ("srt", true, "transcript.srt"),
    ("vtt", false, "subtitles.vtt"),
    ("vtt", true, "transcript.vtt"),
    ("ass", false, "subtitles.ass"),
    ("ass", true, "transcript.ass"),
    ("json", false, "translation.lines.json"),
    ("json", true, "transcript.lines.json"),
    ("txt", false, "translation.txt"),
    ("txt", true, "transcript.txt"),
];

/// Where the lines go. In the project's folder only the fixed name of their
/// kind is written: that folder also holds the studio's own files (source.txt
/// names the video, name.txt the project, import_subs.* are the imported
/// subtitles, analyze's stages are saved as JSON), which a name of the
/// caller's could replace.
fn destination(project_dir: &Path, dir: Option<&str>, name: Option<&str>, format: &str, source: bool) -> Result<PathBuf, String> {
    let fixed = FIXED_NAMES
        .iter()
        .find(|(kind, transcript, _)| *kind == format && *transcript == source)
        .map(|(_, _, fixed)| *fixed)
        .ok_or_else(|| format!("format is one of {}, not {format:?}", FORMATS.join(", ")))?;
    let asked = name.map(str::trim).filter(|n| !n.is_empty());
    match dir.map(str::trim).filter(|d| !d.is_empty()) {
        Some(folder) => {
            let folder = Path::new(folder);
            if !folder.is_dir() {
                return Err(format!("{} is not a folder on this computer", folder.display()));
            }
            Ok(free_name(folder, &file_name(asked.unwrap_or_default(), format, fixed)))
        }
        None => match asked {
            Some(asked) if !file_name(asked, format, fixed).eq_ignore_ascii_case(fixed) => Err(format!(
                "name {asked:?} needs dir: in the project's folder this file is always {fixed}, so that it cannot replace the project's own files (source.txt, name.txt, the imported subtitles, the saved stages of the analysis)"
            )),
            _ => Ok(project_dir.join(fixed)),
        },
    }
}

/// One line of an export: its timing, speaker and words; the recognised line
/// under a translation, and the word timings of a recognised line.
pub(crate) struct Row {
    id: String,
    start: f64,
    end: f64,
    speaker: String,
    text: String,
    original: Option<String>,
    words: Option<Value>,
}

/// The lines the window's buttons export: every line with its translation
/// (the recognised text where it has none), or the recognised lines alone.
pub(crate) fn lines(proj: &Project, source: bool) -> Vec<Row> {
    // The lines render::build_ass burns: no hidden line anywhere, no line that keeps the original
    // speech in the translation, and a caption's own text over the translation.
    let flag = |s: &dub_core::Segment, key: &str| s.extra.get(key).and_then(Value::as_bool).unwrap_or(false);
    let overrides: std::collections::HashMap<&str, &str> = proj
        .captions
        .overrides
        .iter()
        .filter_map(|o| o.text.as_deref().map(|t| (o.seg_id.as_str(), t)))
        .collect();
    proj.segments
        .iter()
        .filter(|s| !flag(s, "hidden") && (source || !flag(s, "keep_original")))
        .filter(|s| !source || !s.src_text.trim().is_empty())
        .map(|s| {
            let tgt = overrides.get(s.id.as_str()).copied().unwrap_or(s.tgt_text.as_str());
            let translated = !source && !tgt.is_empty();
            let text = if translated { tgt } else { s.src_text.as_str() };
            Row {
                id: s.id.clone(),
                start: s.start,
                end: s.end,
                speaker: s.speaker.clone().unwrap_or_else(|| "0".into()),
                text: text.trim().to_string(),
                original: translated.then(|| s.src_text.trim().to_string()),
                words: if source { s.extra.get("words").filter(|words| words.is_array()).cloned() } else { None },
            }
        })
        .collect()
}

/// SRT time: hh:mm:ss,mmm, rounded to the millisecond.
fn srt_time(seconds: f64) -> String {
    let ms = (seconds * 1000.0).round().max(0.0) as u64;
    format!("{:02}:{:02}:{:02},{:03}", ms / 3_600_000, ms % 3_600_000 / 60_000, ms % 60_000 / 1000, ms % 1000)
}

fn srt(rows: &[Row]) -> String {
    rows.iter()
        .enumerate()
        .map(|(i, r)| format!("{}\n{} --> {}\n{}\n", i + 1, srt_time(r.start), srt_time(r.end), r.text))
        .collect::<Vec<_>>()
        .join("\n")
}

fn txt(rows: &[Row], label: &str) -> String {
    rows.iter().map(|r| format!("[{label} {}] {}", r.speaker, r.text)).collect::<Vec<_>>().join("\n")
}

/// WebVTT time: hh:mm:ss.mmm.
fn vtt_time(seconds: f64) -> String {
    srt_time(seconds).replace(',', ".")
}

/// Cue text as WebVTT reads it: markup characters escaped, and no blank line,
/// which would end the cue.
fn vtt_text(text: &str) -> String {
    let escaped = text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    escaped.lines().map(str::trim).filter(|line| !line.is_empty()).collect::<Vec<_>>().join("\n")
}

/// WebVTT; with more than one speaker each cue names its speaker in a voice tag.
fn vtt(rows: &[Row], label: &str) -> String {
    let speakers: std::collections::HashSet<&str> = rows.iter().map(|r| r.speaker.as_str()).collect();
    let voices = speakers.len() > 1;
    let mut out = String::from("WEBVTT\n");
    for r in rows {
        let voice = if voices { format!("<v {} {}>", vtt_text(label), vtt_text(&r.speaker)) } else { String::new() };
        out.push_str(&format!("\n{} --> {}\n{voice}{}\n", vtt_time(r.start), vtt_time(r.end), vtt_text(&r.text)));
    }
    out
}

/// The lines as JSON: id, timing, speaker and text, the recognised line under
/// a translation (original), and the word timings of a recognised line.
fn json_rows(rows: &[Row]) -> Value {
    Value::Array(
        rows.iter()
            .map(|r| {
                let mut line = json!({ "id": r.id, "start": r.start, "end": r.end, "speaker": r.speaker, "text": r.text });
                if let Some(original) = &r.original {
                    line["original"] = original.clone().into();
                }
                if let Some(words) = &r.words {
                    line["words"] = words.clone();
                }
                line
            })
            .collect(),
    )
}

/// A file name without path separators or characters Windows refuses, with
/// the format's extension.
fn file_name(asked: &str, format: &str, default: &str) -> String {
    let safe: String = asked.chars().filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '.' | '_' | '-' | '(' | ')')).collect();
    let safe = safe.trim().trim_matches('.').trim().to_string();
    if safe.is_empty() {
        return default.to_string();
    }
    if safe.to_lowercase().ends_with(&format!(".{format}")) {
        safe
    } else {
        format!("{safe}.{format}")
    }
}

/// The name in the folder, or with (2), (3) when it is taken.
fn free_name(folder: &Path, name: &str) -> PathBuf {
    let first = folder.join(name);
    if !first.exists() {
        return first;
    }
    let path = Path::new(name);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(name);
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or_default();
    (2u32..)
        .map(|n| folder.join(format!("{stem} ({n}).{ext}")))
        .find(|candidate| !candidate.exists())
        .expect("an unbounded range finds a free name")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project {
        serde_json::from_value(json!({
            "segments": [
                { "id": "s1", "start": 0.0, "end": 1.5004, "speaker": "0", "src_text": " Hello ", "tgt_text": "Привет" },
                { "id": "u2", "start": 3661.25, "end": 3662.0, "speaker": "1", "src_text": "", "tgt_text": " Своя фраза " },
                { "id": "s3", "start": 5.0, "end": 6.0, "src_text": "Bye", "tgt_text": "" },
            ]
        }))
        .unwrap()
    }

    #[test]
    fn srt_time_is_the_window_format() {
        assert_eq!(srt_time(0.0), "00:00:00,000");
        assert_eq!(srt_time(1.5004), "00:00:01,500");
        assert_eq!(srt_time(3661.25), "01:01:01,250");
        assert_eq!(srt_time(-2.0), "00:00:00,000");
    }

    #[test]
    fn the_translation_is_every_line_and_the_transcript_the_recognised_ones() {
        let p = project();
        assert_eq!(
            srt(&lines(&p, false)),
            "1\n00:00:00,000 --> 00:00:01,500\nПривет\n\n2\n01:01:01,250 --> 01:01:02,000\nСвоя фраза\n\n3\n00:00:05,000 --> 00:00:06,000\nBye\n"
        );
        assert_eq!(srt(&lines(&p, true)), "1\n00:00:00,000 --> 00:00:01,500\nHello\n\n2\n00:00:05,000 --> 00:00:06,000\nBye\n");
        assert_eq!(txt(&lines(&p, true), "Speaker"), "[Speaker 0] Hello\n[Speaker 0] Bye");
    }

    fn spoken() -> Project {
        serde_json::from_value(json!({
            "meta": { "video": "C:/v/clip.mp4", "duration": 8.0, "width": 1280, "height": 720, "fps": 25.0 },
            "segments": [
                { "id": "s0", "start": 0.25, "end": 1.5, "speaker": "0", "src_text": "Tom & <Jerry>", "tgt_text": "Том и Джерри",
                  "words": [{ "word": "Tom", "start": 0.25, "end": 0.5 }, { "word": "&", "start": 0.5, "end": 0.6 }] },
                { "id": "s1", "start": 2.0, "end": 3.25, "speaker": "1", "src_text": "Run -->\n\nnow", "tgt_text": "" },
            ]
        }))
        .unwrap()
    }

    #[test]
    fn webvtt_escapes_its_markup_and_names_the_speakers() {
        let p = spoken();
        assert_eq!(
            render_text(&lines(&p, true), "vtt", "Speaker"),
            "WEBVTT\n\n00:00:00.250 --> 00:00:01.500\n<v Speaker 0>Tom &amp; &lt;Jerry&gt;\n\n00:00:02.000 --> 00:00:03.250\n<v Speaker 1>Run --&gt;\nnow\n"
        );
        let one: Project = serde_json::from_value(json!({ "segments": [{ "id": "a", "start": 0.0, "end": 1.0, "speaker": "0", "src_text": "Hi" }] })).unwrap();
        assert_eq!(render_text(&lines(&one, true), "vtt", "Speaker"), "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\nHi\n", "one speaker needs no voice tag");
    }

    #[test]
    fn json_keeps_the_speakers_the_word_timings_and_the_original() {
        let p = spoken();
        let transcript: Value = serde_json::from_str(&render_text(&lines(&p, true), "json", "Speaker")).unwrap();
        assert_eq!(transcript[0], json!({ "id": "s0", "start": 0.25, "end": 1.5, "speaker": "0", "text": "Tom & <Jerry>", "words": [{ "word": "Tom", "start": 0.25, "end": 0.5 }, { "word": "&", "start": 0.5, "end": 0.6 }] }));
        assert_eq!(transcript[1]["speaker"], "1");
        let translation: Value = serde_json::from_str(&render_text(&lines(&p, false), "json", "Speaker")).unwrap();
        assert_eq!(translation[0], json!({ "id": "s0", "start": 0.25, "end": 1.5, "speaker": "0", "text": "Том и Джерри", "original": "Tom & <Jerry>" }));
        assert!(translation[1].get("original").is_none(), "a line without a translation is its recognised text");
        assert!(translation[0].get("words").is_none(), "word timings belong to the recognised words");
    }

    #[test]
    fn ass_is_the_burned_style_and_the_transcript_leaves_the_titles_out() {
        let folder = tempfile::tempdir().unwrap();
        let fonts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fonts");
        let mut p = spoken();
        p.captions.titles.push(serde_json::from_value(json!({ "text": "SIGN", "tgt": "ВЫВЕСКА", "bbox": [100, 80, 300, 60], "start": 0.0, "end": 8.0 })).unwrap());
        let subtitles = write_ass(&p, false, &folder.path().join("subtitles.ass"), &fonts, folder.path()).unwrap();
        assert!(subtitles.contains("[Events]") && subtitles.contains("Том и Джерри") && subtitles.contains("ВЫВЕСКА"), "{subtitles}");
        let transcript = write_ass(&p, true, &folder.path().join("transcript.ass"), &fonts, folder.path()).unwrap();
        assert!(transcript.contains("Tom & <Jerry>") && !transcript.contains("Том и Джерри"), "{transcript}");
        assert!(!transcript.contains("ВЫВЕСКА"), "the titles are the picture's translation");
    }

    #[test]
    fn hidden_lines_stay_out_and_a_caption_keeps_its_own_text() {
        let folder = tempfile::tempdir().unwrap();
        let fonts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fonts");
        let mut p: Project = serde_json::from_value(json!({
            "meta": { "duration": 8.0, "width": 1280, "height": 720, "fps": 25.0 },
            "segments": [
                { "id": "a", "start": 0.0, "end": 1.0, "speaker": "0", "src_text": "Hello", "tgt_text": "Привет" },
                { "id": "b", "start": 1.0, "end": 2.0, "speaker": "0", "src_text": "Thanks for watching", "tgt_text": "Спасибо за просмотр", "hidden": true },
                { "id": "c", "start": 2.0, "end": 3.0, "speaker": "0", "src_text": "Bonjour", "tgt_text": "Бонжур", "keep_original": true },
                { "id": "d", "start": 3.0, "end": 4.0, "speaker": "0", "src_text": "Bye", "tgt_text": "Пока" },
            ]
        }))
        .unwrap();
        p.captions.overrides.push(serde_json::from_value(json!({ "seg_id": "d", "text": "До встречи" })).unwrap());
        for format in ["srt", "vtt", "json", "txt"] {
            let translation = render_text(&lines(&p, false), format, "Speaker");
            assert!(translation.contains("Привет") && translation.contains("До встречи"), "{format}: {translation}");
            assert!(!translation.contains("Спасибо") && !translation.contains("Бонжур") && !translation.contains("Пока"), "{format}: {translation}");
            let transcript = render_text(&lines(&p, true), format, "Speaker");
            assert!(transcript.contains("Hello") && transcript.contains("Bonjour") && transcript.contains("Bye"), "{format}: {transcript}");
            assert!(!transcript.contains("Thanks for watching"), "{format}: {transcript}");
        }
        let subtitles = write_ass(&p, false, &folder.path().join("subtitles.ass"), &fonts, folder.path()).unwrap();
        assert!(subtitles.contains("Привет") && subtitles.contains("До встречи"), "{subtitles}");
        assert!(!subtitles.contains("Спасибо") && !subtitles.contains("Бонжур") && !subtitles.contains("Пока"), "{subtitles}");
        let transcript = write_ass(&p, true, &folder.path().join("transcript.ass"), &fonts, folder.path()).unwrap();
        assert!(transcript.contains("Hello") && transcript.contains("Bonjour") && transcript.contains("Bye"), "{transcript}");
        assert!(!transcript.contains("Thanks for watching"), "{transcript}");
    }

    #[test]
    fn every_format_has_its_fixed_name() {
        let folder = tempfile::tempdir().unwrap();
        let named = |format: &str, source: bool| destination(folder.path(), None, None, format, source).unwrap().file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!((named("vtt", false), named("vtt", true)), ("subtitles.vtt".into(), "transcript.vtt".into()));
        assert_eq!((named("ass", false), named("ass", true)), ("subtitles.ass".into(), "transcript.ass".into()));
        assert_eq!((named("json", false), named("json", true)), ("translation.lines.json".into(), "transcript.lines.json".into()));
        assert!(destination(folder.path(), None, Some("project"), "json", true).is_err(), "project.json is the studio's own");
        assert!(destination(folder.path(), None, Some("transcript"), "json", true).is_err(), "transcript.json is analyze's own");
        assert_eq!(destination(folder.path(), None, Some("transcript.lines"), "json", true).unwrap(), folder.path().join("transcript.lines.json"));
        for format in FORMATS {
            assert!(destination(folder.path(), None, None, format, false).is_ok() && destination(folder.path(), None, None, format, true).is_ok(), "{format}");
        }
    }

    #[test]
    fn no_fixed_name_is_a_file_the_studio_keeps_in_the_project_folder() {
        let own = crate::analyze::STAGE_FILES.iter().copied().chain([
            "project.json",
            "source.txt",
            "name.txt",
            "caps.ass",
            "_preview.ass",
            crate::job_store::FILE,
            crate::render::SEG_CKPT_FILE,
            crate::dub_timing::FILE,
            crate::atomic::AGENT_FILE,
            crate::atomic::REGIONS_FILE,
        ]);
        for file in own {
            assert!(!FIXED_NAMES.iter().any(|(_, _, fixed)| fixed.eq_ignore_ascii_case(file)), "an export would replace {file}");
        }
    }

    #[tokio::test]
    async fn the_texts_are_the_exports_not_the_analysis_stages() {
        let root = tempfile::tempdir().unwrap();
        let st = AppState::new(root.path());
        let dir = st.workspace.join("p1");
        std::fs::create_dir_all(&dir).unwrap();
        for file in crate::analyze::STAGE_FILES.iter().copied().chain(["project.json", "transcript.lines.json", "translation.lines.json", "subtitles.srt"]) {
            std::fs::write(dir.join(file), "{}").unwrap();
        }
        let response = files(State(st.clone()), AxPath("p1".into())).await;
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
        let listed: Value = serde_json::from_slice(&bytes).unwrap();
        let names: Vec<String> = listed["texts"].as_array().unwrap().iter().map(|path| Path::new(path.as_str().unwrap()).file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["subtitles.srt", "transcript.lines.json", "translation.lines.json"]);
    }

    #[test]
    fn a_name_is_safe_and_carries_its_extension() {
        assert_eq!(file_name("", "srt", "subtitles.srt"), "subtitles.srt");
        assert_eq!(file_name("..\\..\\evil", "srt", "x.srt"), "evil.srt");
        assert_eq!(file_name("Episode 01 (ru).SRT", "srt", "x.srt"), "Episode 01 (ru).SRT");
        assert_eq!(file_name("notes", "txt", "x.txt"), "notes.txt");
    }

    #[test]
    fn the_project_folder_takes_only_the_fixed_names() {
        let project = tempfile::tempdir().unwrap();
        let folder = project.path();
        std::fs::write(folder.join("source.txt"), "D:/videos/clip.mp4").unwrap();
        let refused = destination(folder, None, Some("source"), "txt", true).unwrap_err();
        assert!(refused.contains("needs dir") && refused.contains("transcript.txt"), "{refused}");
        assert!(destination(folder, Some("  "), Some("name"), "txt", false).is_err(), "a blank dir is no dir");
        assert!(destination(folder, None, Some("import_subs"), "srt", false).is_err());
        assert_eq!(destination(folder, None, None, "srt", false).unwrap(), folder.join("subtitles.srt"));
        assert_eq!(destination(folder, None, Some(" "), "srt", true).unwrap(), folder.join("transcript.srt"));
        assert_eq!(destination(folder, None, Some("Subtitles.SRT"), "srt", false).unwrap(), folder.join("subtitles.srt"));
        assert_eq!(destination(folder, None, None, "txt", false).unwrap(), folder.join("translation.txt"));
        assert_eq!(std::fs::read_to_string(folder.join("source.txt")).unwrap(), "D:/videos/clip.mp4");
    }

    #[test]
    fn a_name_of_ones_own_in_a_folder_never_replaces_a_file() {
        let project = tempfile::tempdir().unwrap();
        let folder = project.path();
        std::fs::write(folder.join("source.txt"), "D:/videos/clip.mp4").unwrap();
        let path = folder.to_str().unwrap();
        assert_eq!(destination(folder, Some(path), Some("source"), "txt", true).unwrap(), folder.join("source (2).txt"));
        assert_eq!(destination(folder, Some(path), None, "srt", false).unwrap(), folder.join("subtitles.srt"));
        assert!(destination(folder, Some("Z:/nowhere/at/all"), Some("x"), "srt", false).unwrap_err().contains("not a folder"));
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let folder = tempfile::tempdir().unwrap();
        assert_eq!(free_name(folder.path(), "a.srt"), folder.path().join("a.srt"));
        std::fs::write(folder.path().join("a.srt"), "x").unwrap();
        std::fs::write(folder.path().join("a (2).srt"), "x").unwrap();
        assert_eq!(free_name(folder.path(), "a.srt"), folder.path().join("a (3).srt"));
    }
}
