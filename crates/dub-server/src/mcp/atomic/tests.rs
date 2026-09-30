use serde_json::{json, Value};

use super::super::tests::{answer_text, call_tool, stub};
use super::stub::{self as studio, Ends, Plan};
use super::ROUTES;

fn media(pid: &str) -> String {
    format!("C:/media/{pid}.mp4")
}

/// A tool's answer: its structured result, or its error text.
async fn answer(tool: &str, arguments: Value) -> Result<Value, String> {
    stub();
    let reply = call_tool(tool, arguments).await;
    if reply["result"]["isError"] == true {
        return Err(answer_text(&reply));
    }
    Ok(reply["result"]["structuredContent"].clone())
}

fn asked(pid: &str, what: &str) -> usize {
    studio::log(pid).iter().filter(|entry| entry.starts_with(what)).count()
}

#[tokio::test]
async fn a_transcript_is_one_call_and_the_second_answers_from_the_project() {
    studio::plan("tr1", Plan::default());
    let first = answer("transcribe_file", json!({ "path": media("tr1"), "format": "srt" })).await.unwrap();
    assert_eq!((first["done"].clone(), first["project_id"].clone(), first["speakers"].clone()), (json!(true), json!("tr1"), json!(2)), "{first}");
    assert_eq!(first["transcript"], "1\n00:00:00,500 --> 00:00:01,500\nHello there\n\n2\n00:00:02,000 --> 00:00:03,000\nBye now\n");
    assert_eq!(first["file"], "C:/w/tr1/export.srt");
    let log = studio::log("tr1");
    let settings = "tgt_lang=en&mode=transcribe&src_lang=auto&subs=transcribe&detect=0&casting=0";
    assert!(log.contains(&format!("POST /projects/from-path key=transcribe_file?{settings} tool=transcribe_file")), "{log:?}");
    assert!(log.contains(&format!("POST /projects/tr1/analyze?{settings}")), "{log:?}");
    assert!(log.contains(&"POST /projects/tr1/export-text srt src".to_string()), "{log:?}");

    studio::forget_log("tr1");
    let again = answer("transcribe_file", json!({ "path": media("tr1"), "format": "srt" })).await.unwrap();
    assert_eq!(again["transcript"], first["transcript"]);
    assert_eq!(asked("tr1", "POST /projects/tr1/analyze"), 0, "the finished analysis is not run again: {:?}", studio::log("tr1"));
}

#[tokio::test]
async fn a_transcript_without_speakers_is_the_subtitles_mode_in_its_own_language() {
    studio::plan("tr2", Plan::default());
    let found = answer("transcribe_file", json!({ "path": media("tr2"), "src_lang": "DE", "diarize": false, "format": "json" })).await.unwrap();
    assert!(studio::log("tr2").contains(&"POST /projects/tr2/analyze?tgt_lang=de&mode=nodub&src_lang=de&subs=transcribe&detect=0&casting=0".to_string()), "{:?}", studio::log("tr2"));
    assert_eq!(found["speakers"], 1);
    let lines = found["transcript"].as_array().unwrap();
    assert_eq!(lines[0]["text"], "Hello there");
    assert_eq!(lines[0]["words"][1], json!({ "word": "there", "start": 0.75, "end": 1.0 }));
    let refused = answer("transcribe_file", json!({ "path": media("tr2"), "format": "doc" })).await.unwrap_err();
    assert!(refused.contains("format is one of text, srt, vtt, json"), "{refused}");
    let refused = answer("transcribe_file", json!({ "path": media("tr2"), "src_lang": "xx" })).await.unwrap_err();
    assert!(refused.contains("not a language code"), "{refused}");
}

#[tokio::test]
async fn translated_subtitles_carry_the_tone_asked_for() {
    studio::plan("tl1", Plan::default());
    let found = answer("translate_file", json!({ "path": media("tl1"), "tgt_lang": "es", "style": "very formal", "format": "vtt" })).await.unwrap();
    assert!(
        studio::log("tl1").contains(&"POST /projects/tl1/analyze?tgt_lang=es&mode=nodub&src_lang=auto&subs=translate&translate_style=very%20formal&detect=0&casting=0".to_string()),
        "{:?}",
        studio::log("tl1")
    );
    assert_eq!(found["subtitles"], "WEBVTT\n\n00:00:00.500 --> 00:00:01.500\n[es] Hello there\n\n00:00:02.000 --> 00:00:03.000\n[es] Bye now\n");
    assert_eq!(found["language"], "es");
    let same = answer("translate_file", json!({ "path": media("tl1"), "tgt_lang": "en", "src_lang": "en" })).await.unwrap_err();
    assert!(same.contains("transcribe_file"), "{same}");
    let bare = answer("translate_file", json!({ "path": media("tl1") })).await.unwrap_err();
    assert!(bare.contains("'tgt_lang' is required"), "{bare}");

    let subtitles = answer("export_subtitles", json!({ "project_id": "tl1", "format": "vtt" })).await.unwrap();
    assert_eq!((subtitles["text"].clone(), subtitles["path"].clone()), (json!("translation"), json!("C:/w/tl1/export.vtt")));
    let original = answer("export_subtitles", json!({ "project_id": "tl1", "lang": "original", "dir": "D:/out", "name": "clip" })).await.unwrap();
    assert_eq!((original["text"].clone(), original["path"].clone(), original["format"].clone()), (json!("transcript"), json!("D:/out/clip.srt"), json!("srt")));
    assert_eq!(answer("export_subtitles", json!({ "project_id": "tl1", "lang": "ES" })).await.unwrap()["text"], "translation");
    let other = answer("export_subtitles", json!({ "project_id": "tl1", "lang": "fr" })).await.unwrap_err();
    assert!(other.contains("neither") && other.contains("translate_file"), "{other}");
    assert!(studio::log("tl1").contains(&"POST /projects/tl1/export-text srt src".to_string()));
}

#[tokio::test]
async fn subtitles_of_a_project_without_lines_are_refused() {
    studio::plan("ex1", Plan::default());
    let refused = answer("export_subtitles", json!({ "project_id": "ex1" })).await.unwrap_err();
    assert!(refused.contains("has no lines yet"), "{refused}");
    let transcript = answer("transcribe_file", json!({ "path": media("ex1") })).await.unwrap();
    assert_eq!(transcript["transcript"], "[Speaker 0] Hello there\n[Speaker 1] Bye now");
    let other = answer("export_subtitles", json!({ "project_id": "ex1", "lang": "es" })).await.unwrap_err();
    assert!(other.contains("without a translation"), "{other}");
    assert_eq!(answer("export_subtitles", json!({ "project_id": "ex1" })).await.unwrap()["text"], "transcript");
}

#[tokio::test]
async fn a_long_stage_answers_its_job_and_the_next_call_waits_for_it() {
    studio::plan("lg1", Plan { analyze: Ends::Never, ..Plan::default() });
    let first = answer("transcribe_file", json!({ "path": media("lg1"), "seconds": 1 })).await.unwrap();
    assert_eq!((first["done"].clone(), first["stage"].clone(), first["job"]["status"].clone()), (json!(false), json!("analysis"), json!("running")), "{first}");
    let job = first["job"]["id"].as_str().unwrap().to_string();
    assert!(first["next"].as_str().unwrap().contains(&format!("studio_wait with job_id {job}")), "{first}");
    let second = answer("transcribe_file", json!({ "path": media("lg1"), "seconds": 1 })).await.unwrap();
    assert_eq!(second["job"]["id"], job.as_str(), "the running analysis is waited for, not started again");
    assert_eq!(asked("lg1", "POST /projects/lg1/analyze"), 1);
}

#[tokio::test]
async fn an_analysis_already_at_work_is_waited_for_and_a_failed_one_says_why() {
    studio::plan("cf1", Plan { conflict: true, ..Plan::default() });
    let found = answer("transcribe_file", json!({ "path": media("cf1") })).await.unwrap();
    assert_eq!(found["done"], true, "the job the 409 names is waited for");

    studio::plan("fl1", Plan { analyze: Ends::Fails, ..Plan::default() });
    let failed = answer("translate_file", json!({ "path": media("fl1"), "tgt_lang": "ru" })).await.unwrap_err();
    assert!(failed.contains("The analysis failed") && failed.contains("analyze broke on purpose"), "{failed}");
    let refused = answer("transcribe_file", json!({ "path": media("nowhere2") })).await.unwrap_err();
    assert!(refused.contains("is not a file on this computer"), "{refused}");
}

#[tokio::test]
async fn a_dub_runs_every_stage_once_and_keeps_its_copy() {
    let folder = tempfile::tempdir().unwrap();
    let render = folder.path().join("render");
    std::fs::create_dir_all(&render).unwrap();
    let output = render.join("output.mp4");
    std::fs::write(&output, b"the dubbed video").unwrap();
    let out_dir = folder.path().join("out");
    std::fs::create_dir_all(&out_dir).unwrap();
    studio::plan("db1", Plan { output: Some(output.to_string_lossy().into_owned()), degraded: true, ..Plan::default() });
    let arguments = json!({ "path": media("db1"), "tgt_lang": "ru", "voice": "pack:Anna", "keep_original": true, "out_dir": out_dir.to_string_lossy() });
    let dubbed = answer("dub_file", arguments.clone()).await.unwrap();
    assert_eq!(dubbed["done"], true, "{dubbed}");
    let log = studio::log("db1");
    let settings = "tgt_lang=ru&mode=dub&src_lang=auto&subs=translate&burn=1&detect=1&casting=0&keep_original=1&container=mp4";
    assert!(log.contains(&format!("POST /projects/from-path key=dub_file?{settings}&voice=pack%3AAnna tool=dub_file")), "{log:?}");
    assert!(log.contains(&format!("POST /projects/db1/analyze?{settings}")), "{log:?}");
    assert_eq!(log.iter().filter(|entry| entry.starts_with("PATCH /projects/db1") && entry.contains("\"voice_name\":\"Anna\"")).count(), 1, "{log:?}");
    assert_eq!(asked("db1", "PATCH /projects/db1"), 1, "the second track came with the analysis: {log:?}");
    assert_eq!(asked("db1", "POST /projects/db1/render"), 1);
    let saved = out_dir.join("db1.ru.mp4");
    assert_eq!(dubbed["saved"], saved.to_string_lossy().as_ref());
    assert_eq!(std::fs::read(&saved).unwrap(), b"the dubbed video");
    assert_eq!(dubbed["video"], output.to_string_lossy().as_ref());
    assert_eq!(dubbed["stages"], json!({ "analysis": "done now", "voices": { "mode": "voice", "names": "Anna", "second_track": true }, "render": "done now" }));
    assert_eq!(dubbed["degradations"][0]["code"], "no_separator", "{dubbed}");
    assert_eq!(dubbed["degradations"][0]["job"], "render");

    studio::forget_log("db1");
    let again = answer("dub_file", arguments).await.unwrap();
    assert_eq!(again["stages"]["analysis"], "done before");
    assert_eq!(again["stages"]["render"], "done before");
    assert_eq!(again["saved"], dubbed["saved"]);
    for step in ["POST /projects/db1/analyze", "PATCH /projects/db1", "POST /projects/db1/render", "POST /projects/db1/save-output"] {
        assert_eq!(asked("db1", step), 0, "{step} again: {:?}", studio::log("db1"));
    }
}

#[tokio::test]
async fn autocast_goes_with_the_analysis_and_wrong_voices_are_refused() {
    studio::plan("ac1", Plan::default());
    let dubbed = answer("dub_file", json!({ "path": media("ac1"), "tgt_lang": "es", "voice": "autocast", "male_voices": ["Boris"], "female_voices": ["Vera"], "mode": "voiceover" })).await.unwrap();
    let log = studio::log("ac1");
    let analysis = log.iter().find(|entry| entry.starts_with("POST /projects/ac1/analyze")).unwrap();
    assert!(analysis.contains("mode=voiceover") && analysis.contains("voice_slots=") && analysis.contains("Boris") && analysis.contains("Vera"), "{analysis}");
    assert_eq!(asked("ac1", "PATCH /projects/ac1"), 0, "the analysis deals the voices: {log:?}");
    assert_eq!(dubbed["stages"]["voices"]["mode"], "voice");
    assert_eq!(dubbed["degradations"][0], json!({ "job": "analyze", "stage": "voices", "code": "missing_voices", "detail": ["Gone"] }));

    for (arguments, says) in [
        (json!({ "voice": "pack:Nobody" }), "no voice Nobody"),
        (json!({ "voice": "autocast" }), "male_voices and female_voices"),
        (json!({ "voice": "choir" }), "clone, autocast or pack:"),
        (json!({ "male_voices": ["Boris"] }), "go with voice autocast"),
        (json!({ "mode": "subtitles", "voice": "pack:Anna" }), "mode subtitles keeps the original audio"),
        (json!({ "mode": "subtitles", "subs": "none" }), "would be the original"),
        (json!({ "out_dir": "Z:/no/such/folder" }), "not a folder"),
    ] {
        let mut wrong = json!({ "path": media("ac1"), "tgt_lang": "es" });
        wrong.as_object_mut().unwrap().extend(arguments.as_object().unwrap().clone());
        let refused = answer("dub_file", wrong).await.unwrap_err();
        assert!(refused.contains(says), "{arguments}: {refused}");
    }

    studio::plan("au1", Plan { audio: true, ..Plan::default() });
    let refused = answer("dub_file", json!({ "path": media("au1"), "tgt_lang": "es", "mode": "subtitles" })).await.unwrap_err();
    assert!(refused.contains("is audio") && refused.contains("translate_file"), "{refused}");
    assert_eq!(asked("au1", "POST /projects/au1/render"), 0);
}

#[tokio::test]
async fn a_render_still_at_work_is_answered_and_the_next_call_finishes() {
    studio::plan("rn1", Plan { render: Ends::Never, ..Plan::default() });
    let first = answer("dub_file", json!({ "path": media("rn1"), "tgt_lang": "ru", "seconds": 1 })).await.unwrap();
    assert_eq!((first["done"].clone(), first["stage"].clone()), (json!(false), json!("render")), "{first}");
    let second = answer("dub_file", json!({ "path": media("rn1"), "tgt_lang": "ru", "seconds": 1 })).await.unwrap();
    assert_eq!((second["job"]["id"].clone(), second["stage"].clone()), (first["job"]["id"].clone(), json!("render")), "the running render is waited for, named as what it is");
    assert_eq!((asked("rn1", "POST /projects/rn1/analyze"), asked("rn1", "POST /projects/rn1/render")), (1, 1), "{:?}", studio::log("rn1"));
}

#[tokio::test]
async fn voice_and_background_and_the_text_in_the_picture_are_one_call_each() {
    studio::plan("sp1", Plan::default());
    let split = answer("separate_file", json!({ "path": media("sp1") })).await.unwrap();
    assert_eq!(split, json!({ "done": true, "project_id": "sp1", "vocals": "C:/w/sp1/stems/vocals.wav", "background": "C:/w/sp1/stems/instrumental.wav" }));
    assert!(studio::log("sp1").iter().any(|entry| entry == "POST /projects/from-path key=media tool=separate_file"), "{:?}", studio::log("sp1"));
    studio::forget_log("sp1");
    assert_eq!(answer("separate_file", json!({ "path": media("sp1") })).await.unwrap(), split, "the stems are answered from the project");
    assert_eq!(asked("sp1", "GET /jobs/"), 0, "no job the second time: {:?}", studio::log("sp1"));

    let read = answer("detect_text_file", json!({ "path": media("sp1") })).await.unwrap();
    assert_eq!((read["count"].clone(), read["regions"][0]["text"].clone(), read["project_id"].clone()), (json!(1), json!("EXIT"), json!("sp1")), "{read}");
    assert!(read.get("cached").is_none());

    studio::plan("sp2", Plan { stage: Ends::Fails, ..Plan::default() });
    let failed = answer("separate_file", json!({ "path": media("sp2") })).await.unwrap_err();
    assert!(failed.contains("The separation failed") && failed.contains("separate broke on purpose"), "{failed}");
}

/// Every request a one-call tool makes is to a route it declares in ROUTES, which the test of
/// every route checks against the studio's router.
#[tokio::test]
async fn every_one_call_tool_calls_only_the_routes_it_declares() {
    let folder = tempfile::tempdir().unwrap();
    let calls = [
        ("transcribe_file", "rt1", json!({})),
        ("translate_file", "rt2", json!({ "tgt_lang": "ru" })),
        ("dub_file", "rt3", json!({ "tgt_lang": "ru", "voice": "pack:Anna", "out_dir": folder.path().to_string_lossy() })),
        ("separate_file", "rt4", json!({})),
        ("detect_text_file", "rt5", json!({})),
    ];
    for (tool, pid, extra) in calls {
        studio::plan(pid, Plan::default());
        let mut arguments = json!({ "path": media(pid) });
        arguments.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        answer(tool, arguments).await.unwrap_or_else(|why| panic!("{tool}: {why}"));
        check(tool, pid);
    }
    answer("export_subtitles", json!({ "project_id": "rt2", "format": "ass" })).await.unwrap();
    check("export_subtitles", "rt2");

    fn check(tool: &str, pid: &str) {
        let declared = ROUTES.iter().find(|(path, _)| *path == format!("composite:atomic:{tool}")).map(|(_, routes)| *routes).unwrap_or_else(|| panic!("{tool} declares no routes"));
        for entry in studio::log(pid) {
            let (method, rest) = entry.split_once(' ').unwrap();
            let path = rest.split([' ', '?']).next().unwrap();
            let job = |part: &str| part.strip_prefix("job").is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
            let pattern: Vec<&str> = path.split('/').map(|part| if part == pid || job(part) { "x1" } else { part }).collect();
            let pattern = pattern.join("/");
            assert!(declared.contains(&(method, pattern.as_str())), "{tool} called {method} {path}, which it does not declare");
        }
        studio::forget_log(pid);
    }
}

#[test]
fn a_one_call_tool_on_a_file_is_idempotent_and_changes_something() {
    for name in ["transcribe_file", "translate_file", "dub_file", "separate_file", "detect_text_file"] {
        let hints = super::super::annotations(name);
        assert_eq!((hints["readOnlyHint"].clone(), hints["idempotentHint"].clone(), hints["destructiveHint"].clone()), (json!(false), json!(true), json!(false)), "{name}");
    }
    assert_eq!(super::super::annotations("export_subtitles")["readOnlyHint"], false);
}
