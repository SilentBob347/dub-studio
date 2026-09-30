//! Ручки секретов: ключ OpenRouter и пароль прокси. Сами значения наружу не уходят никогда — только
//! «задан ли» и источник. Ошибки — JSON `{error: <код>, detail}`: текст для окна выбирает фронт по коду.

use std::path::Path;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

use crate::credentials::{self, CredentialSource};
use crate::AppState;

fn failure(status: StatusCode, code: &str, detail: impl Into<String>) -> Response {
    (status, Json(json!({ "error": code, "detail": detail.into() }))).into_response()
}

fn openrouter_state() -> Value {
    let source = credentials::openrouter_source();
    json!({
        "configured": source.is_some(),
        "source": source,
        "environment_variable": credentials::OPENROUTER_ENV_VAR,
    })
}

// ─── GET /engine/openrouter/settings ────────────────────────────────────────
pub async fn openrouter_settings() -> Json<Value> {
    Json(openrouter_state())
}

// ─── PUT /engine/openrouter/settings {api_key} ──────────────────────────────
// Ключ сохраняется только после того, как OpenRouter его принял (операция verify сайдкара).
pub async fn update_openrouter_settings(State(st): State<AppState>, Json(body): Json<Value>) -> Response {
    let key = body.get("api_key").and_then(Value::as_str).map(str::trim).unwrap_or_default().to_string();
    if key.is_empty() {
        return failure(StatusCode::BAD_REQUEST, "empty_key", "api_key is empty; DELETE /engine/openrouter/settings removes the key");
    }
    if key.contains(['\r', '\n']) {
        return failure(StatusCode::BAD_REQUEST, "invalid_key", "an OpenRouter API key must be a single line");
    }
    if credentials::openrouter_source() == Some(CredentialSource::Environment) {
        return failure(
            StatusCode::CONFLICT,
            "environment_key",
            format!("{} is set in this environment and takes priority", credentials::OPENROUTER_ENV_VAR),
        );
    }
    let repo = st.repo_root.clone();
    let candidate = key.clone();
    let verified = tokio::task::spawn_blocking(move || crate::openrouter_cli::run_json(&repo, &candidate, "verify", &json!({})))
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
    match verified {
        Err(e) => return failure(StatusCode::BAD_GATEWAY, "verify_failed", e),
        Ok(answer) if answer.get("ok") != Some(&Value::Bool(true)) => {
            let detail = answer.get("error").and_then(Value::as_str).unwrap_or_default().to_string();
            return failure(StatusCode::BAD_REQUEST, "key_rejected", detail);
        }
        Ok(_) => {}
    }
    match credentials::store_openrouter_api_key(Some(&key)) {
        Ok(_) => Json(openrouter_state()).into_response(),
        Err(e) => failure(StatusCode::INTERNAL_SERVER_ERROR, "store_failed", format!("{e:#}")),
    }
}

// ─── DELETE /engine/openrouter/settings ─────────────────────────────────────
pub async fn delete_openrouter_settings() -> Response {
    if credentials::openrouter_source() == Some(CredentialSource::Environment) {
        return failure(
            StatusCode::CONFLICT,
            "environment_key",
            format!("{} is set in this environment; unset it to remove the key", credentials::OPENROUTER_ENV_VAR),
        );
    }
    match credentials::store_openrouter_api_key(None) {
        Ok(_) => Json(openrouter_state()).into_response(),
        Err(e) => failure(StatusCode::INTERNAL_SERVER_ERROR, "store_failed", format!("{e:#}")),
    }
}

/// Прокси как его видит окно: адрес без пароля и флаг «пароль задан».
pub(crate) fn proxy_view(models_root: &Path, secrets: &Path) -> Value {
    let public = crate::models::redact_selection(
        &crate::models::load_selection(models_root),
        false,
        credentials::proxy_password_in(secrets).is_some(),
    );
    json!({
        "on": public.get("proxy_on").and_then(Value::as_str) == Some("1"),
        "url": public.get("proxy_url").and_then(Value::as_str).unwrap_or_default(),
        "password_set": public["proxy_password_set"],
    })
}

#[derive(Debug)]
pub(crate) struct FormError {
    status: StatusCode,
    code: &'static str,
    detail: String,
}

impl FormError {
    fn bad(code: &'static str, detail: impl Into<String>) -> Self {
        FormError { status: StatusCode::BAD_REQUEST, code, detail: detail.into() }
    }
    fn internal(detail: impl std::fmt::Display) -> Self {
        FormError { status: StatusCode::INTERNAL_SERVER_ERROR, code: "store_failed", detail: detail.to_string() }
    }
}

/// Сохранить форму прокси `{on?, url?, password?}`. Пароль: нет поля или пустая строка — оставить
/// сохранённый (форма его не знает), null — удалить, строка — заменить. Пароль, вписанный прямо в адрес,
/// тоже уходит в хранилище; в active.json адрес попадает всегда без пароля.
pub(crate) fn apply_proxy_form(models_root: &Path, secrets: &Path, form: &Value) -> Result<(), FormError> {
    let change = match form.get("password") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(password)) if password.trim().is_empty() => None,
        Some(Value::String(password)) => Some(Some(password.trim().to_string())),
        Some(_) => return Err(FormError::bad("invalid_proxy_password", "password must be a string or null")),
    };
    let on = match form.get("on") {
        None => None,
        Some(Value::Bool(on)) => Some(*on),
        Some(_) => return Err(FormError::bad("invalid_proxy_on", "on must be a boolean")),
    };

    let mut selection = crate::models::load_selection(models_root);
    let slots = selection.as_object_mut().expect("load_selection returns object");
    match form.get("url") {
        None => {
            let stored = slots.get("proxy_url").and_then(Value::as_str).unwrap_or_default();
            if matches!(change, Some(Some(_))) && !crate::models::proxy_has_user(stored) {
                return Err(FormError::bad("proxy_password_without_user", "a proxy password needs a user name in the address (user@host:port)"));
            }
            if let Some(change) = change {
                credentials::store_proxy_password_in(secrets, change.as_deref()).map_err(FormError::internal)?;
            }
        }
        Some(Value::String(url)) if url.trim().is_empty() => {
            slots.remove("proxy_url");
            credentials::store_proxy_password_in(secrets, None).map_err(FormError::internal)?;
        }
        Some(Value::String(url)) => {
            let (bare, inline) = crate::models::split_proxy_password(url.trim());
            let change = change.or(inline.map(Some));
            if crate::models::proxy_has_user(&bare) {
                if let Some(change) = change {
                    credentials::store_proxy_password_in(secrets, change.as_deref()).map_err(FormError::internal)?;
                }
            } else {
                if matches!(change, Some(Some(_))) {
                    return Err(FormError::bad("proxy_password_without_user", "a proxy password needs a user name in the address (user@host:port)"));
                }
                credentials::store_proxy_password_in(secrets, None).map_err(FormError::internal)?;
            }
            slots.insert("proxy_url".into(), Value::String(bare));
        }
        Some(_) => return Err(FormError::bad("invalid_proxy_url", "url must be a string")),
    }
    if let Some(on) = on {
        slots.insert("proxy_on".into(), Value::String(if on { "1" } else { "0" }.into()));
    }
    crate::models::write_selection(models_root, &selection).map_err(FormError::internal)
}

// ─── GET /engine/proxy/settings ─────────────────────────────────────────────
pub async fn proxy_settings(State(st): State<AppState>) -> Response {
    let Some(secrets) = credentials::secrets_dir() else {
        return failure(StatusCode::INTERNAL_SERVER_ERROR, "no_secrets_dir", "no per-user application data directory for credential storage");
    };
    Json(proxy_view(&st.models_root, &secrets)).into_response()
}

// ─── PUT /engine/proxy/settings {on?, url?, password?} ──────────────────────
pub async fn update_proxy_settings(State(st): State<AppState>, Json(form): Json<Value>) -> Response {
    let Some(secrets) = credentials::secrets_dir() else {
        return failure(StatusCode::INTERNAL_SERVER_ERROR, "no_secrets_dir", "no per-user application data directory for credential storage");
    };
    match apply_proxy_form(&st.models_root, &secrets, &form) {
        Ok(()) => Json(proxy_view(&st.models_root, &secrets)).into_response(),
        Err(e) => failure(e.status, e.code, e.detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::Request;
    use axum::http::header;
    use tower::ServiceExt;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("dub-secrets-api-{tag}-{}-{}", std::process::id(), uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn resolved(models: &Path, secrets: &Path) -> String {
        let bare = crate::models::load_selection(models)["proxy_url"].as_str().unwrap().to_string();
        crate::models::proxy_with_password(&bare, credentials::proxy_password_in(secrets).as_deref())
    }

    #[test]
    fn saving_the_proxy_form_unchanged_keeps_the_password() {
        let models = scratch("form-models");
        let secrets = scratch("form-secrets");
        apply_proxy_form(&models, &secrets, &json!({ "on": true, "url": "http://alice:hunter2@proxy.lan:3128" })).unwrap();
        let active = std::fs::read_to_string(models.join("active.json")).unwrap();
        assert!(!active.contains("hunter2"));
        assert_eq!(resolved(&models, &secrets), "http://alice:hunter2@proxy.lan:3128");

        let view = proxy_view(&models, &secrets);
        assert_eq!(view, json!({ "on": true, "url": "http://alice@proxy.lan:3128", "password_set": true }));

        for untouched in [
            json!({ "on": view["on"], "url": view["url"] }),
            json!({ "on": view["on"], "url": view["url"], "password": "" }),
            json!({ "on": false }),
        ] {
            apply_proxy_form(&models, &secrets, &untouched).unwrap();
            assert_eq!(resolved(&models, &secrets), "http://alice:hunter2@proxy.lan:3128", "{untouched}");
        }
        assert_eq!(proxy_view(&models, &secrets)["on"], false);

        apply_proxy_form(&models, &secrets, &json!({ "url": "http://alice@proxy2.lan:3128", "password": "n3w" })).unwrap();
        assert_eq!(resolved(&models, &secrets), "http://alice:n3w@proxy2.lan:3128");

        apply_proxy_form(&models, &secrets, &json!({ "password": null })).unwrap();
        assert_eq!(proxy_view(&models, &secrets)["password_set"], false);
        assert_eq!(resolved(&models, &secrets), "http://alice@proxy2.lan:3128");

        let refused = apply_proxy_form(&models, &secrets, &json!({ "url": "socks5://proxy.lan:1080", "password": "x" }));
        assert_eq!(refused.err().map(|e| e.code), Some("proxy_password_without_user"));

        apply_proxy_form(&models, &secrets, &json!({ "url": "http://bob:pw@proxy.lan:8080" })).unwrap();
        apply_proxy_form(&models, &secrets, &json!({ "url": "" })).unwrap();
        assert!(crate::models::load_selection(&models).get("proxy_url").is_none());
        assert_eq!(credentials::proxy_password_in(&secrets), None);

        std::fs::remove_dir_all(&models).unwrap();
        std::fs::remove_dir_all(&secrets).unwrap();
    }

    async fn body_text(app: &axum::Router, request: Request) -> (StatusCode, String) {
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    fn local(method: &str, uri: &str, body: Option<Value>) -> Request {
        let builder = Request::builder().method(method).uri(uri).header(header::HOST, "127.0.0.1:8793");
        match body {
            Some(body) => builder.header(header::CONTENT_TYPE, "application/json").body(Body::from(body.to_string())).unwrap(),
            None => builder.body(Body::empty()).unwrap(),
        }
    }

    #[tokio::test]
    async fn no_response_carries_a_secret() {
        let root = scratch("leak-root");
        std::fs::create_dir_all(root.join("models")).unwrap();
        std::fs::write(
            root.join("models").join("active.json"),
            r#"{"or_key":"sk-or-v1-leaktest","proxy_on":"1","proxy_url":"http://alice:hunter2@proxy.lan:3128","bench":"1"}"#,
        )
        .unwrap();
        let app = crate::build_router(crate::AppState::new(&root));

        let on_disk = std::fs::read_to_string(root.join("models").join("active.json")).unwrap();
        assert!(!on_disk.contains("sk-or-v1-leaktest") && !on_disk.contains("hunter2"), "migrated on start: {on_disk}");

        let (status, capabilities) = body_text(&app, local("GET", "/engine/capabilities", None)).await;
        assert_eq!(status, StatusCode::OK);
        let parsed: Value = serde_json::from_str(&capabilities).unwrap();
        assert_eq!(parsed["selection"]["or_key_set"], true);
        assert_eq!(parsed["selection"]["proxy_password_set"], true);
        assert_eq!(parsed["selection"]["proxy_url"], "http://alice@proxy.lan:3128");

        let (status, selected) = body_text(&app, local("POST", "/engine/select", Some(json!({ "key": "bench", "value": "0" })))).await;
        assert_eq!(status, StatusCode::OK);
        let (status, by_component) = body_text(&app, local("POST", "/engine/select", Some(json!({ "id": "higgs-q6_k" })))).await;
        assert_eq!(status, StatusCode::OK);
        let (_, openrouter) = body_text(&app, local("GET", "/engine/openrouter/settings", None)).await;
        let (_, proxy) = body_text(&app, local("GET", "/engine/proxy/settings", None)).await;
        for answer in [&capabilities, &selected, &by_component, &openrouter, &proxy] {
            assert!(!answer.contains("sk-or-v1-leaktest") && !answer.contains("hunter2"), "{answer}");
        }
        assert_eq!(serde_json::from_str::<Value>(&openrouter).unwrap()["configured"], true);
        assert_eq!(serde_json::from_str::<Value>(&proxy).unwrap()["password_set"], true);

        let (status, _) = body_text(&app, local("POST", "/engine/select", Some(json!({ "key": "or_key", "value": "sk-x" })))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = body_text(&app, local("POST", "/engine/select", Some(json!({ "key": "proxy_url", "value": "http://a:b@h:1" })))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, empty) = body_text(&app, local("PUT", "/engine/openrouter/settings", Some(json!({ "api_key": " " })))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(empty.contains("empty_key"));

        if std::env::var(credentials::OPENROUTER_ENV_VAR).is_err() {
            let (status, removed) = body_text(&app, local("DELETE", "/engine/openrouter/settings", None)).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(serde_json::from_str::<Value>(&removed).unwrap()["configured"], false);
        }

        std::fs::remove_dir_all(&root).unwrap();
        if let Some(dir) = credentials::secrets_dir() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
