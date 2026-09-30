//! История дублей фразы: до `MAX_TAKES` последних озвучек реплики в `takes/<sid>/<n>.wav` и
//! `takes/<sid>/takes.json`. Активный дубль — тот, что лежит в `seg_<sid>.wav` и чей ключ синтеза записан в
//! seg_ckpt.json: его берёт микс, выбор другого дубля меняет только пересборку микса. Закреплённый дубль
//! рендер не заменяет, пока текст реплики тот, что в нём озвучен.

use std::path::{Path, PathBuf};

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::AppState;

pub const DIR: &str = "takes";
pub const FILE: &str = "takes.json";
pub const MAX_TAKES: usize = 5;

/// Одна озвучка реплики.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Take {
    pub n: u32,
    /// Озвученный текст.
    pub text: String,
    /// Ключ синтеза (render::seg_key): совпал с ключом реплики — дубль годен без нового синтеза.
    pub key: String,
    /// Нонс «перегенерировать» реплики на момент синтеза (Segment.extra.regen).
    #[serde(default)]
    pub nonce: Option<Value>,
    /// Голос: clone, имя голоса из библиотеки или облачный голос.
    pub voice: String,
    /// Файл референса клона.
    pub reference: String,
    /// Параметры синтеза.
    pub params: String,
    /// Откуда дубль: synth, multitake, qc, shorten.
    pub source: String,
    /// Длительность клипа, сек.
    pub dur: f64,
    /// Сходство услышанного ASR с текстом (QC), если проверялось.
    #[serde(default)]
    pub qc: Option<f64>,
    /// Время создания, секунды эпохи.
    pub created: u64,
}

/// Что известно о новой озвучке, кроме её файла.
pub struct NewTake {
    pub text: String,
    pub key: String,
    pub nonce: Option<Value>,
    pub voice: String,
    pub reference: String,
    pub params: String,
    pub source: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct History {
    #[serde(default)]
    pub next: u32,
    #[serde(default)]
    pub active: Option<u32>,
    #[serde(default)]
    pub pinned: Option<u32>,
    #[serde(default)]
    pub takes: Vec<Take>,
}

fn dir_of(wd: &Path, sid: &str) -> PathBuf {
    wd.join(DIR).join(sid)
}

pub fn wav(wd: &Path, sid: &str, n: u32) -> PathBuf {
    dir_of(wd, sid).join(format!("{n}.wav"))
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl History {
    /// Нет файла — пустая история; битый — ошибка с причиной.
    pub fn load(wd: &Path, sid: &str) -> Result<Self, String> {
        let p = dir_of(wd, sid).join(FILE);
        match std::fs::read_to_string(&p) {
            Ok(t) => serde_json::from_str(&t).map_err(|e| format!("разбор {}: {e}", p.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(History::default()),
            Err(e) => Err(format!("чтение {}: {e}", p.display())),
        }
    }

    pub fn save(&self, wd: &Path, sid: &str) -> Result<(), String> {
        let d = dir_of(wd, sid);
        std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        let body = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        dub_core::atomic::write(&d.join(FILE), &body)
    }

    pub fn get(&self, n: u32) -> Option<&Take> {
        self.takes.iter().find(|t| t.n == n)
    }

    pub fn pinned_take(&self) -> Option<&Take> {
        self.pinned.and_then(|n| self.get(n))
    }

    /// Самый свежий дубль с этим ключом синтеза.
    pub fn by_key(&self, key: &str) -> Option<&Take> {
        self.takes.iter().rev().find(|t| t.key == key)
    }

    /// Положить клип `src` новым дублем, сделать его активным и сохранить историю. Сверх `MAX_TAKES`
    /// удаляются самые старые дубли, кроме активного и закреплённого.
    pub fn add(&mut self, wd: &Path, sid: &str, src: &Path, t: NewTake, dur: f64) -> Result<u32, String> {
        let n = self.next.max(self.takes.iter().map(|t| t.n + 1).max().unwrap_or(0));
        let d = dir_of(wd, sid);
        std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        dub_core::atomic::copy(src, &wav(wd, sid, n))?;
        self.takes.push(Take {
            n,
            text: t.text,
            key: t.key,
            nonce: t.nonce,
            voice: t.voice,
            reference: t.reference,
            params: t.params,
            source: t.source.to_string(),
            dur,
            qc: None,
            created: now_secs(),
        });
        self.next = n + 1;
        self.active = Some(n);
        self.prune(wd, sid)?;
        self.save(wd, sid)?;
        Ok(n)
    }

    fn prune(&mut self, wd: &Path, sid: &str) -> Result<(), String> {
        while self.takes.len() > MAX_TAKES {
            let keep = |t: &Take| Some(t.n) == self.active || Some(t.n) == self.pinned;
            let Some(pos) = self.takes.iter().enumerate().filter(|(_, t)| !keep(t)).min_by_key(|(_, t)| t.n).map(|(i, _)| i) else {
                break;
            };
            let gone = self.takes.remove(pos);
            let f = wav(wd, sid, gone.n);
            match std::fs::remove_file(&f) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("удаление старого дубля {}: {e}", f.display())),
            }
        }
        Ok(())
    }

    /// Положить дубль `n` в файл сегмента и сделать активным (история сохраняется).
    pub fn restore(&mut self, wd: &Path, sid: &str, n: u32, seg_wav: &Path) -> Result<(), String> {
        if self.get(n).is_none() {
            return Err(format!("дубля {n} нет в истории фразы {sid}"));
        }
        dub_core::atomic::copy(&wav(wd, sid, n), seg_wav)?;
        if self.active != Some(n) {
            self.active = Some(n);
            self.save(wd, sid)?;
        }
        Ok(())
    }

    pub fn set_qc(&mut self, wd: &Path, sid: &str, n: u32, sim: f64) -> Result<(), String> {
        match self.takes.iter_mut().find(|t| t.n == n) {
            Some(t) => {
                t.qc = Some(sim);
                self.save(wd, sid)
            }
            None => Ok(()),
        }
    }

    /// Сводка для строки редактора и агента.
    pub fn summary(&self) -> Value {
        json!({ "count": self.takes.len(), "active": self.active, "pinned": self.pinned })
    }
}

/// Снять закрепление, если текст реплики теперь не тот, что озвучен в закреплённом дубле. true — снято.
pub fn unpin_if_stale(wd: &Path, sid: &str, text: &str) -> Result<bool, String> {
    let mut h = History::load(wd, sid)?;
    match h.pinned_take() {
        Some(p) if p.text != text.trim() => {
            h.pinned = None;
            h.save(wd, sid)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Сводки историй всех фраз проекта: sid -> {count, active, pinned}.
pub fn summaries(wd: &Path) -> Result<std::collections::HashMap<String, Value>, String> {
    let root = wd.join(DIR);
    let mut out = std::collections::HashMap::new();
    let rd = match std::fs::read_dir(&root) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(format!("{}: {e}", root.display())),
    };
    for e in rd {
        let e = e.map_err(|e| format!("{}: {e}", root.display()))?;
        if !e.path().is_dir() {
            continue;
        }
        let sid = e.file_name().to_string_lossy().into_owned();
        let h = History::load(wd, &sid)?;
        if !h.takes.is_empty() {
            out.insert(sid, h.summary());
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- правки проекта (PATCH take_select / take_pin)

pub fn is_take_op(op: &str) -> bool {
    matches!(op, "take_select" | "take_pin")
}

fn seg_sid(proj: &dub_core::Project, id: &str) -> Result<String, (u16, String)> {
    if !proj.segments.iter().any(|s| s.id == id) {
        return Err((404, format!("segment {id:?} not found")));
    }
    crate::render::seg_file_id(id).ok_or((400, format!("segment id {id:?} has no file name")))
}

fn edit_id(edit: &Value) -> Result<String, (u16, String)> {
    edit.get("id").and_then(Value::as_str).map(str::to_string).ok_or((400, "missing segment id".into()))
}

/// Дополнить правку take_select тем, что знает история: текст, нонс и ключ выбранного дубля (их ставит
/// patch::apply в реплику).
pub fn resolve(wd: &Path, proj: &dub_core::Project, edit: &Value) -> Result<Value, (u16, String)> {
    let op = edit.get("op").and_then(Value::as_str).unwrap_or_default();
    let id = edit_id(edit)?;
    let sid = seg_sid(proj, &id)?;
    let h = History::load(wd, &sid).map_err(|e| (500, e))?;
    let mut out = edit.clone();
    match op {
        "take_select" => {
            let n = edit.get("take").and_then(Value::as_u64).ok_or((400, "take_select needs take (its n from takes_list)".to_string()))?;
            let t = u32::try_from(n).ok().and_then(|n| h.get(n)).ok_or((404, format!("take {n} of segment {id:?} not found")))?;
            out["take_text"] = t.text.clone().into();
            out["take_nonce"] = t.nonce.clone().unwrap_or(Value::Null);
            out["take_key"] = t.key.clone().into();
        }
        "take_pin" => {
            let pin = edit.get("pinned").and_then(Value::as_bool).ok_or((400, "take_pin needs pinned (true or false)".to_string()))?;
            if pin {
                let active = h.active.and_then(|n| h.get(n)).ok_or((409, format!("segment {id:?} has no voiced take to pin")))?;
                let seg = proj.segments.iter().find(|s| s.id == id).expect("seg_sid checked it");
                if active.text != seg.tgt_text.trim() {
                    return Err((409, format!("the active take of {id:?} voices other text: voice the line again or pick its take first")));
                }
            }
        }
        other => return Err((400, format!("{other:?} is not a take op"))),
    }
    Ok(out)
}

/// Файловая часть правки после сохранения проекта: выбранный дубль — в файл сегмента и seg_ckpt.json,
/// закрепление — в историю.
pub fn commit(wd: &Path, proj: &dub_core::Project, edit: &Value) -> Result<(), String> {
    let op = edit.get("op").and_then(Value::as_str).unwrap_or_default();
    let id = edit_id(edit).map_err(|(_, m)| m)?;
    let sid = seg_sid(proj, &id).map_err(|(_, m)| m)?;
    let mut h = History::load(wd, &sid)?;
    match op {
        "take_select" => {
            let n = edit.get("take").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()).ok_or("take_select without take")?;
            let key = h.get(n).map(|t| t.key.clone()).ok_or_else(|| format!("take {n} of segment {id:?} not found"))?;
            h.restore(wd, &sid, n, &wd.join(format!("seg_{sid}.wav")))?;
            let mut ck = crate::render::SegCkpts::load(wd)?;
            ck.set(&sid, &key)?;
            Ok(())
        }
        "take_pin" => {
            let pin = edit.get("pinned").and_then(Value::as_bool).unwrap_or(false);
            h.pinned = if pin { h.active } else { None };
            h.save(wd, &sid)
        }
        other => Err(format!("{other:?} is not a take op")),
    }
}

// ---------------------------------------------------------------- маршруты

fn take_row(wd: &Path, sid: &str, t: &Take, current: &str) -> Value {
    json!({
        "n": t.n, "text": t.text, "text_matches": t.text == current.trim(), "dur": t.dur, "qc": t.qc,
        "source": t.source, "voice": t.voice, "reference": t.reference, "params": t.params, "created": t.created,
        "file": wav(wd, sid, t.n).to_string_lossy(),
    })
}

/// GET /projects/{pid}/segments/{id}/takes — дубли фразы, активный и закреплённый.
pub async fn list(State(st): State<AppState>, AxPath((pid, id)): AxPath<(String, String)>) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let proj = match st.load_project(&pid) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let sid = match seg_sid(&proj, &id) {
        Ok(s) => s,
        Err((code, msg)) => return (StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_REQUEST), msg).into_response(),
    };
    let current = proj.segments.iter().find(|s| s.id == id).map(|s| s.tgt_text.clone()).unwrap_or_default();
    match History::load(&dir, &sid) {
        Ok(h) => {
            let rows: Vec<Value> = h.takes.iter().rev().map(|t| take_row(&dir, &sid, t, &current)).collect();
            Json(json!({ "id": id, "active": h.active, "pinned": h.pinned, "takes": rows })).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

/// GET /projects/{pid}/segments/{id}/takes/{n}/audio — клип дубля для прослушивания.
pub async fn audio(
    State(st): State<AppState>,
    AxPath((pid, id, n)): AxPath<(String, String, u32)>,
    req: axum::http::Request<axum::body::Body>,
) -> Response {
    let dir = match st.proj_dir(&pid) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let proj = match st.load_project(&pid) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let sid = match seg_sid(&proj, &id) {
        Ok(s) => s,
        Err((code, msg)) => return (StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_REQUEST), msg).into_response(),
    };
    match History::load(&dir, &sid) {
        Ok(h) if h.get(n).is_some() => crate::serve_file_range(&wav(&dir, &sid, n), req, None).await,
        Ok(_) => (StatusCode::NOT_FOUND, format!("take {n} of segment {id:?} not found")).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wd(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dub_takes_{tag}_{}_{}", std::process::id(), uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn take(text: &str, key: &str) -> NewTake {
        NewTake {
            text: text.into(),
            key: key.into(),
            nonce: None,
            voice: "clone".into(),
            reference: "ref_spk0.wav".into(),
            params: "{}".into(),
            source: "synth",
        }
    }

    fn clip(d: &Path, name: &str, body: &[u8]) -> PathBuf {
        let p = d.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn history_keeps_five_and_never_drops_the_pinned_one() {
        let d = wd("prune");
        let mut h = History::default();
        let first = h.add(&d, "s0", &clip(&d, "a.wav", b"a"), take("Привет", "k0"), 1.0).unwrap();
        h.pinned = Some(first);
        for i in 1..=6u8 {
            h.add(&d, "s0", &clip(&d, "a.wav", &[i]), take("Привет", &format!("k{i}")), 1.0).unwrap();
        }
        assert_eq!(h.takes.len(), MAX_TAKES);
        assert!(h.get(first).is_some(), "the pinned take stays");
        assert_eq!(h.active, Some(6));
        assert!(!wav(&d, "s0", 1).exists() && !wav(&d, "s0", 2).exists(), "the oldest unpinned takes are gone with their files");
        assert!(wav(&d, "s0", 6).exists());
        let back = History::load(&d, "s0").unwrap();
        assert_eq!(back, h);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn restore_puts_the_take_into_the_segment_file() {
        let d = wd("restore");
        let mut h = History::default();
        let a = h.add(&d, "s1", &clip(&d, "a.wav", b"AAA"), take("Один", "ka"), 1.0).unwrap();
        h.add(&d, "s1", &clip(&d, "b.wav", b"BBB"), take("Два", "kb"), 1.0).unwrap();
        let seg = d.join("seg_s1.wav");
        h.restore(&d, "s1", a, &seg).unwrap();
        assert_eq!(std::fs::read(&seg).unwrap(), b"AAA");
        assert_eq!(History::load(&d, "s1").unwrap().active, Some(a));
        assert_eq!(h.by_key("kb").map(|t| t.n), Some(1));
        assert!(h.restore(&d, "s1", 9, &seg).is_err());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_changed_text_unpins() {
        let d = wd("unpin");
        let mut h = History::default();
        h.add(&d, "s2", &clip(&d, "a.wav", b"A"), take("Привет", "k"), 1.0).unwrap();
        h.pinned = h.active;
        h.save(&d, "s2").unwrap();
        assert!(!unpin_if_stale(&d, "s2", " Привет ").unwrap());
        assert!(unpin_if_stale(&d, "s2", "Пока").unwrap());
        assert_eq!(History::load(&d, "s2").unwrap().pinned, None);
        let all = summaries(&d).unwrap();
        assert_eq!(all["s2"]["count"], 1);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn broken_history_is_an_error() {
        let d = wd("broken");
        std::fs::create_dir_all(d.join(DIR).join("s3")).unwrap();
        std::fs::write(d.join(DIR).join("s3").join(FILE), b"{").unwrap();
        assert!(History::load(&d, "s3").unwrap_err().contains("takes.json"));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn take_select_is_resolved_and_committed() {
        let d = wd("select");
        let mut proj = dub_core::Project::default();
        proj.segments.push(dub_core::Segment { id: "s4".into(), tgt_text: "Новый".into(), ..Default::default() });
        let mut h = History::default();
        let old = h.add(&d, "s4", &clip(&d, "a.wav", b"OLD"), NewTake { nonce: Some(json!("n1")), ..take("Старый", "k-old") }, 1.0).unwrap();
        h.add(&d, "s4", &clip(&d, "b.wav", b"NEW"), take("Новый", "k-new"), 1.0).unwrap();
        let edit = json!({ "op": "take_select", "id": "s4", "take": old });
        let resolved = resolve(&d, &proj, &edit).unwrap();
        assert_eq!(resolved["take_text"], "Старый");
        assert_eq!(resolved["take_nonce"], "n1");
        assert_eq!(resolved["take_key"], "k-old");
        commit(&d, &proj, &resolved).unwrap();
        assert_eq!(std::fs::read(d.join("seg_s4.wav")).unwrap(), b"OLD");
        assert_eq!(crate::render::SegCkpts::load(&d).unwrap().get("s4"), Some("k-old"));
        assert_eq!(resolve(&d, &proj, &json!({ "op": "take_select", "id": "s4", "take": 7 })).unwrap_err().0, 404);
        assert_eq!(resolve(&d, &proj, &json!({ "op": "take_select", "id": "zz", "take": 0 })).unwrap_err().0, 404);
        assert_eq!(resolve(&d, &proj, &json!({ "op": "take_pin", "id": "s4", "pinned": true })).unwrap_err().0, 409, "the active take voices other text");
        proj.segments[0].tgt_text = "Старый".into();
        let pin = json!({ "op": "take_pin", "id": "s4", "pinned": true });
        resolve(&d, &proj, &pin).unwrap();
        commit(&d, &proj, &pin).unwrap();
        assert_eq!(History::load(&d, "s4").unwrap().pinned, Some(old));
        std::fs::remove_dir_all(&d).unwrap();
    }
}
