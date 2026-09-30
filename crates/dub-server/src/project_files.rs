//! A project's files for an agent: where they are on this computer, and its
//! lines written as SRT or plain text the way the window's export buttons
//! write them, into any folder and without opening Explorer.

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
                    matches!(ext.as_str(), "srt" | "txt" | "ass") && file != "source.txt" && file != "name.txt"
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
        "texts": texts,
    }))
    .into_response()
}

/// POST /projects/{pid}/export-text {format: srt|txt, text?: tgt|src, dir?, name?, speaker_label?}
/// — write the lines as a file. Without dir it goes into the project's folder
/// under the fixed name of its kind, replacing the earlier one, as the window's
/// save-text does; a name of one's own needs dir, and there a name already
/// taken gets (2), (3) instead of being overwritten.
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
    if !matches!(format, "srt" | "txt") {
        return (StatusCode::BAD_REQUEST, format!("format is srt or txt, not {format:?}")).into_response();
    }
    let which = body.get("text").and_then(Value::as_str).unwrap_or("tgt");
    let source = match which {
        "tgt" => false,
        "src" => true,
        other => return (StatusCode::BAD_REQUEST, format!("text is tgt or src, not {other:?}")).into_response(),
    };
    let rows = lines(&proj, source);
    let content = if format == "srt" {
        srt(&rows)
    } else {
        let label = body.get("speaker_label").and_then(Value::as_str).map(str::trim).filter(|l| !l.is_empty()).unwrap_or("Speaker");
        txt(&rows, label)
    };
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
    if let Err(e) = std::fs::write(&target, content) {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("write {}: {e}", target.display())).into_response();
    }
    Json(json!({ "ok": true, "path": target.to_string_lossy(), "lines": rows.len() })).into_response()
}

/// Where the lines go. In the project's folder only the fixed name of their
/// kind is written: that folder also holds the studio's own text files
/// (source.txt names the video, name.txt the project, import_subs.* are the
/// imported subtitles), which a name of the caller's could replace.
fn destination(project_dir: &Path, dir: Option<&str>, name: Option<&str>, format: &str, source: bool) -> Result<PathBuf, String> {
    let fixed = match (format, source) {
        ("srt", false) => "subtitles.srt",
        ("srt", true) => "transcript.srt",
        (_, false) => "translation.txt",
        (_, true) => "transcript.txt",
    };
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
                "name {asked:?} needs dir: in the project's folder this file is always {fixed}, so that it cannot replace the project's own files (source.txt, name.txt, the imported subtitles)"
            )),
            _ => Ok(project_dir.join(fixed)),
        },
    }
}

/// One line of an export: its timing, speaker and words.
struct Row {
    start: f64,
    end: f64,
    speaker: String,
    text: String,
}

/// The lines the window's buttons export: every line with its translation
/// (the recognised text where it has none), or the recognised lines alone.
fn lines(proj: &Project, source: bool) -> Vec<Row> {
    proj.segments
        .iter()
        .filter(|s| !source || !s.src_text.trim().is_empty())
        .map(|s| {
            let text = if source || s.tgt_text.is_empty() { &s.src_text } else { &s.tgt_text };
            Row { start: s.start, end: s.end, speaker: s.speaker.clone().unwrap_or_else(|| "0".into()), text: text.trim().to_string() }
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
