//! OpenAI-совместимый чат-клиент (/v1/chat/completions) к трём видам серверов:
//! * свой llama-server (Gemma + mmproj) — путь паритета с питоном: sampling как в translate.py, content
//!   как есть (<think> снимает вызывающий), `chat_template_kwargs.enable_thinking=false`;
//! * локальный OpenAI-совместимый сервер пользователя (Ollama, LM Studio, vLLM, llama-server) — адрес, модель
//!   и необязательный ключ; рассуждения вырезаются из ответа, обрезанный сервером промпт — ошибка;
//! * OpenRouter — модель из каталога: параметры, которые модель принимает, и управление рассуждениями.
//!
//! content — строка ИЛИ массив частей (text / image_url data:base64 / input_audio) для мультимодальности.

use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};

use crate::openrouter::ModelProfile;
use crate::LlmError;

/// Часть сообщения content. Соответствует list-форме content в питоне:
///   {"type":"text","text":...} | {"type":"image_url","image_url":{"url":"data:image/png;base64,..."}}
///   | {"type":"input_audio","input_audio":{"data":<base64>,"format":"wav"}}
#[derive(Clone)]
pub enum Part {
    Text(String),
    /// PNG-кадр как base64 (без префикса) — обёрнём в data:image/png;base64,.
    ImagePngB64(String),
    /// WAV-аудио как base64 (без префикса) + формат ("wav").
    AudioB64 { data: String, format: String },
}

impl Part {
    fn to_value(&self) -> Value {
        match self {
            Part::Text(t) => json!({"type":"text","text": t}),
            Part::ImagePngB64(b64) => json!({
                "type":"image_url",
                "image_url": {"url": format!("data:image/png;base64,{b64}")}
            }),
            Part::AudioB64 { data, format } => json!({
                "type":"input_audio",
                "input_audio": {"data": data, "format": format}
            }),
        }
    }
}

/// Роль + content одного сообщения. content — либо строка, либо массив частей (мультимодальность).
#[derive(Clone)]
pub struct Message {
    pub role: String,
    pub parts_text: Option<String>, // строковый content (быстрый путь для чисто текстовых вызовов)
    pub parts: Option<Vec<Part>>,   // массив частей (image/audio/text)
}

impl Message {
    /// Текстовое сообщение с заданной ролью (общий конструктор system/user_text).
    fn text_msg(role: &str, text: impl Into<String>) -> Self {
        Message {
            role: role.into(),
            parts_text: Some(text.into()),
            parts: None,
        }
    }
    pub fn system(text: impl Into<String>) -> Self {
        Self::text_msg("system", text)
    }
    pub fn user_text(text: impl Into<String>) -> Self {
        Self::text_msg("user", text)
    }
    pub fn user_parts(parts: Vec<Part>) -> Self {
        Message {
            role: "user".into(),
            parts_text: None,
            parts: Some(parts),
        }
    }

    fn to_value(&self) -> Value {
        let content = if let Some(t) = &self.parts_text {
            Value::String(t.clone())
        } else if let Some(ps) = &self.parts {
            Value::Array(ps.iter().map(|p| p.to_value()).collect())
        } else {
            Value::String(String::new())
        };
        json!({"role": self.role, "content": content})
    }

    /// Символов текста в сообщении (картинки и аудио не считаются).
    fn text_chars(&self) -> usize {
        let parts = self.parts.iter().flatten().map(|part| match part {
            Part::Text(text) => text.chars().count(),
            _ => 0,
        });
        self.parts_text.as_deref().map_or(0, |text| text.chars().count()) + parts.sum::<usize>()
    }
}

/// Sampling-параметры одного вызова. Дефолты нейтральны; вызывающий выставляет ровно то, что в питоне.
#[derive(Clone, Serialize)]
pub struct Sampling {
    pub temperature: f32,
    pub top_p: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_penalty: Option<f32>,
    pub max_tokens: u32,
}

impl Sampling {
    pub fn new(temperature: f32, top_p: f32, max_tokens: u32) -> Self {
        Sampling {
            temperature,
            top_p,
            top_k: None,
            repeat_penalty: None,
            max_tokens,
        }
    }
    pub fn top_k(mut self, k: i32) -> Self {
        self.top_k = Some(k);
        self
    }
    pub fn repeat_penalty(mut self, r: f32) -> Self {
        self.repeat_penalty = Some(r);
        self
    }
}

/// К какому серверу обращается клиент.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endpoint {
    /// Свой llama-server на 127.0.0.1 (без поля model, одна модель).
    LlamaServer,
    /// Локальный OpenAI-совместимый сервер пользователя (Ollama, LM Studio, vLLM, llama-server).
    OpenAiCompatible,
    /// OpenRouter.
    OpenRouter,
}

/// Сколько токенов добавить к max_tokens модели, которая обязана думать: рассуждение считается в max_tokens.
const REASONING_ROOM: u32 = 4096;
/// Потолок max_tokens при повторе обрезанного ответа.
const MAX_TOKENS_CEILING: u32 = 32768;

/// Клиент чата. Держит blocking reqwest client с большим таймаутом (генерация целого транскрипта/vision-кадра
/// может идти десятки секунд).
pub struct ChatClient {
    /// Адрес сервера без хвоста `/v1` — к нему дописывается `/v1/chat/completions`.
    base_url: String,
    http: reqwest::blocking::Client,
    retries: u32,
    endpoint: Endpoint,
    /// Bearer-токен; None у llama-server и у своего сервера без ключа.
    auth: Option<String>,
    /// id модели в теле запроса. None у llama-server (единственная модель).
    model: Option<String>,
    /// Запись каталога OpenRouter выбранной модели (что она принимает и как думает).
    profile: Option<ModelProfile>,
}

/// Текст ошибки reqwest вместе с цепочкой причин: верхний уровень («error sending request») не
/// говорит, что именно сломалось (отказ соединения, сброс, таймаут).
fn error_chain(e: &dyn std::error::Error) -> String {
    let mut out = e.to_string();
    let mut src = e.source();
    while let Some(s) = src {
        out.push_str(": ");
        out.push_str(&s.to_string());
        src = s.source();
    }
    out
}

/// Адрес сервера без `/` и `/v1` в конце: пользователи пишут и так, и так.
fn server_base(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string()
}

impl ChatClient {
    fn build(base_url: String, endpoint: Endpoint) -> Result<Self, LlmError> {
        let builder = match endpoint {
            // Свой llama-server — всегда мимо прокси, в том числе из переменных окружения.
            Endpoint::LlamaServer => crate::net::local_builder(),
            Endpoint::OpenAiCompatible | Endpoint::OpenRouter => crate::net::builder(),
        };
        let http = builder
            .timeout(Duration::from_secs(600)) // целый транскрипт/vision-кадр может генериться долго
            .build()
            .map_err(|e| LlmError::Http(error_chain(&e)))?;
        Ok(ChatClient { base_url, http, retries: 2, endpoint, auth: None, model: None, profile: None })
    }

    /// Клиент к своему llama-server (base_url = http://127.0.0.1:PORT).
    pub fn new(base_url: impl Into<String>) -> Result<Self, LlmError> {
        Self::build(base_url.into(), Endpoint::LlamaServer)
    }

    /// Клиент к локальному OpenAI-совместимому серверу пользователя: адрес (с `/v1` или без), модель из его
    /// `/v1/models`, необязательный ключ.
    pub fn openai_compatible(base_url: &str, model: impl Into<String>, api_key: Option<String>) -> Result<Self, LlmError> {
        let mut c = Self::build(server_base(base_url), Endpoint::OpenAiCompatible)?;
        c.model = Some(model.into());
        c.auth = api_key.map(|key| key.trim().to_string()).filter(|key| !key.is_empty());
        Ok(c)
    }

    /// Клиент к OpenRouter: ключ + id модели.
    pub fn openrouter(api_key: impl Into<String>, model: impl Into<String>) -> Result<Self, LlmError> {
        Self::openrouter_at("https://openrouter.ai/api", api_key, model)
    }

    /// Как `openrouter`, но с другим адресом API (тесты).
    pub fn openrouter_at(base_url: &str, api_key: impl Into<String>, model: impl Into<String>) -> Result<Self, LlmError> {
        let mut c = Self::build(server_base(base_url), Endpoint::OpenRouter)?;
        c.auth = Some(api_key.into());
        c.model = Some(model.into());
        Ok(c)
    }

    /// Учесть запись каталога OpenRouter выбранной модели.
    pub fn with_profile(mut self, profile: Option<ModelProfile>) -> Self {
        self.profile = profile;
        self
    }

    pub fn with_retries(mut self, n: u32) -> Self {
        self.retries = n;
        self
    }

    pub fn endpoint(&self) -> Endpoint {
        self.endpoint
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// Тело запроса для этого сервера.
    fn body(&self, messages: &[Message], s: &Sampling, max_tokens: u32) -> Value {
        let mut body = serde_json::Map::new();
        if let Some(m) = &self.model {
            body.insert("model".into(), json!(m));
        }
        body.insert("messages".into(), Value::Array(messages.iter().map(|m| m.to_value()).collect()));
        let mut max_tokens = max_tokens;
        match self.endpoint {
            Endpoint::LlamaServer | Endpoint::OpenAiCompatible => {
                body.insert("temperature".into(), json!(s.temperature));
                body.insert("top_p".into(), json!(s.top_p));
                // top_k/repeat_penalty ВСТАВЛЯЕМ только когда заданы: llama-server отвергает явный null (400).
                // Имена llama.cpp — их же понимают LM Studio и Ollama; незнакомые поля серверы пропускают.
                if let Some(k) = s.top_k {
                    body.insert("top_k".into(), json!(k));
                }
                if let Some(rp) = s.repeat_penalty {
                    body.insert("repeat_penalty".into(), json!(rp));
                }
                if self.endpoint == Endpoint::LlamaServer {
                    // enable_thinking=false — как Gemma4ChatHandler(enable_thinking=False): без него Gemma-4
                    // сжигает max_tokens на reasoning_content, content пустой. Это специфика своего llama-server.
                    body.insert("chat_template_kwargs".into(), json!({"enable_thinking": false}));
                }
            }
            Endpoint::OpenRouter => {
                // В облако — только то, что модель заявила в supported_parameters, под именами OpenRouter.
                // Сэмплинг перевода (выверен на тест-сете) главнее; чего перевод не задаёт — берётся из
                // default_parameters, которые модель публикует для себя.
                let empty = ModelProfile::default();
                let profile = self.profile.as_ref().unwrap_or(&empty);
                let defaults = &profile.defaults;
                let ours = [
                    ("temperature", Some(json!(s.temperature))),
                    ("top_p", Some(json!(s.top_p))),
                    ("top_k", s.top_k.map(|k| json!(k)).or(defaults.top_k.map(|k| json!(k)))),
                    ("repetition_penalty", s.repeat_penalty.map(|r| json!(r)).or(defaults.repetition_penalty.map(|r| json!(r)))),
                    ("frequency_penalty", defaults.frequency_penalty.map(|v| json!(v))),
                    ("presence_penalty", defaults.presence_penalty.map(|v| json!(v))),
                ];
                for (name, value) in ours {
                    let known = self.profile.is_some() || matches!(name, "temperature" | "top_p");
                    if let (Some(value), true) = (value, known && profile.accepts(name)) {
                        body.insert(name.into(), value);
                    }
                }
                if let Some(profile) = &self.profile {
                    // Переводу рассуждения не нужны: где их можно выключить — выключаем; модели, которая
                    // обязана думать, — самое низкое усилие, мысли не возвращать, бюджет с запасом.
                    if let Some(reasoning) = &profile.reasoning {
                        if reasoning.mandatory {
                            let mut control = json!({ "exclude": true });
                            if let Some(effort) = reasoning.effort_for(Some("low")) {
                                control["effort"] = json!(effort);
                            }
                            body.insert("reasoning".into(), control);
                            max_tokens = max_tokens.saturating_add(REASONING_ROOM);
                        } else {
                            body.insert("reasoning".into(), json!({ "enabled": false }));
                        }
                    }
                }
            }
        }
        body.insert("max_tokens".into(), json!(max_tokens));
        body.insert("stream".into(), json!(false));
        Value::Object(body)
    }

    /// Один вызов /v1/chat/completions -> текст ассистента. Ретраит на сетевых/5xx/429-ошибках.
    /// llama-server: content как есть (пустой ответ — не ошибка, <think> снимает вызывающий).
    /// Свой сервер и OpenRouter: ответ без рассуждений; ответ, обрезанный лимитом токенов, повторяется
    /// с удвоенным лимитом, затем — ошибка `CutShort`; промпт, обрезанный сервером, — ошибка `PromptCut`.
    pub fn chat(&self, messages: &[Message], s: &Sampling) -> Result<String, LlmError> {
        let mut max_tokens = s.max_tokens;
        let mut cut_retries = if self.endpoint == Endpoint::LlamaServer { 0 } else { 1 };
        loop {
            let body = self.body(messages, s, max_tokens);
            let answer = self.send(&body)?;
            if self.endpoint == Endpoint::LlamaServer {
                return Ok(answer
                    .pointer("/choices/0/message/content")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_string());
            }
            if self.endpoint == Endpoint::OpenAiCompatible {
                let prompt_chars: usize = messages.iter().map(Message::text_chars).sum();
                if let Some(read) = answer.pointer("/usage/prompt_tokens").and_then(Value::as_u64) {
                    if read < (prompt_chars / 6) as u64 {
                        return Err(LlmError::PromptCut(format!(
                            "сервер прочитал только {read} токенов из запроса в {prompt_chars} символов и отбросил начало — \
                             его контекст мал: увеличьте num_ctx в Ollama или Context Length модели в LM Studio"
                        )));
                    }
                }
            }
            let message = answer.pointer("/choices/0/message").cloned().unwrap_or(Value::Null);
            let text = crate::answer::content_of(&message);
            let finish = answer.pointer("/choices/0/finish_reason").and_then(Value::as_str).unwrap_or_default();
            if finish != "length" {
                return Ok(text);
            }
            if cut_retries == 0 || max_tokens >= MAX_TOKENS_CEILING {
                let model = self.model.as_deref().unwrap_or("?");
                return Err(LlmError::CutShort(format!(
                    "модель {model} упёрлась в лимит {max_tokens} токенов (finish_reason=length) — ответ неполный; \
                     вероятно, она тратит бюджет на рассуждения: выберите модель без обязательного мышления"
                )));
            }
            cut_retries -= 1;
            max_tokens = max_tokens.saturating_mul(2).min(MAX_TOKENS_CEILING);
        }
    }

    /// POST с ретраями; успешный ответ — JSON.
    fn send(&self, body: &Value) -> Result<Value, LlmError> {
        let url = format!("{}/v1/chat/completions", self.base_url);
        let mut last_err = String::new();
        for attempt in 0..=self.retries {
            let mut req = self.http.post(&url).json(body);
            if let Some(key) = &self.auth {
                req = req.bearer_auth(key);
            }
            if self.endpoint == Endpoint::OpenRouter {
                req = req.header("HTTP-Referer", "https://github.com/timoncool/dub-studio").header("X-Title", "Dub Studio");
            }
            match req.send() {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        return resp.json::<Value>().map_err(|e| LlmError::Http(format!("parse json: {}", error_chain(&e))));
                    }
                    // 4xx (кроме 429) — не ретраим, это наша ошибка запроса.
                    let text = resp.text().unwrap_or_default();
                    if status.as_u16() != 429 && status.as_u16() < 500 {
                        return Err(LlmError::Api(format!("{status}: {text}")));
                    }
                    last_err = format!("{status}: {text}");
                }
                Err(e) => last_err = error_chain(&e),
            }
            if attempt < self.retries {
                std::thread::sleep(Duration::from_millis(500 * (attempt as u64 + 1)));
            }
        }
        Err(LlmError::Api(format!(
            "chat failed after {} retries: {last_err}",
            self.retries + 1
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openrouter::{ModelDefaults, ReasoningSupport};
    use crate::test_http::{body_json, serve, Reply};

    fn answer(content: &str, finish: &str) -> Reply {
        Reply::json(200, &json!({ "choices": [{ "message": { "content": content }, "finish_reason": finish }] }).to_string())
    }

    #[test]
    fn a_local_server_gets_its_model_and_key_without_llama_kwargs() {
        let server = serve(vec![answer("<think>hm</think>1. Привет", "stop")]);
        let client = ChatClient::openai_compatible(&format!("{}/v1/", server.base()), "gemma3:12b", Some("secret".into())).unwrap();
        let out = client.chat(&[Message::user_text("ping")], &Sampling::new(0.2, 0.9, 32).top_k(20)).unwrap();
        assert_eq!(out, "1. Привет");
        let request = server.request(0);
        assert!(request.starts_with("POST /v1/chat/completions "), "{request}");
        assert!(request.to_ascii_lowercase().contains("authorization: bearer secret"));
        let body = body_json(&request);
        assert_eq!(body["model"], "gemma3:12b");
        assert_eq!(body["top_k"], 20);
        assert!(body.get("chat_template_kwargs").is_none());
    }

    #[test]
    fn a_local_server_without_a_key_sends_no_authorization() {
        let server = serve(vec![answer("ok", "stop")]);
        let client = ChatClient::openai_compatible(&server.base(), "m", Some("  ".into())).unwrap();
        assert_eq!(client.chat(&[Message::user_text("ping")], &Sampling::new(0.0, 1.0, 8)).unwrap(), "ok");
        assert!(!server.request(0).to_ascii_lowercase().contains("authorization:"));
    }

    #[test]
    fn the_own_llama_server_keeps_the_python_path() {
        let server = serve(vec![answer("<think>x</think>raw", "length")]);
        let client = ChatClient::new(server.base()).unwrap();
        let out = client.chat(&[Message::user_text("ping")], &Sampling::new(0.2, 0.95, 64).top_k(64).repeat_penalty(1.05)).unwrap();
        assert_eq!(out, "<think>x</think>raw", "content as is, strip_think is the caller's");
        let body = body_json(&server.request(0));
        assert!(body.get("model").is_none());
        assert_eq!(body["chat_template_kwargs"], json!({ "enable_thinking": false }));
        assert_eq!(body["repeat_penalty"], json!(1.05f32));
        assert_eq!(server.count(), 1, "a cut local answer is not retried");
    }

    #[test]
    fn openrouter_sends_only_what_the_model_takes_and_turns_thinking_off() {
        let server = serve(vec![answer("1. Hola", "stop")]);
        let profile = ModelProfile {
            supported_parameters: vec!["temperature".into(), "top_p".into(), "repetition_penalty".into(), "reasoning".into()],
            reasoning: Some(ReasoningSupport { default_enabled: Some(true), ..ReasoningSupport::default() }),
            defaults: ModelDefaults::default(),
        };
        let client = ChatClient::openrouter_at(&server.base(), "sk-or", "vendor/model").unwrap().with_profile(Some(profile));
        let out = client.chat(&[Message::user_text("ping")], &Sampling::new(0.2, 0.95, 100).top_k(20).repeat_penalty(1.05)).unwrap();
        assert_eq!(out, "1. Hola");
        let request = server.request(0);
        assert!(request.starts_with("POST /v1/chat/completions "));
        let body = body_json(&request);
        assert_eq!(body["model"], "vendor/model");
        assert!(body.get("top_k").is_none(), "the model does not list top_k");
        assert!(body.get("repeat_penalty").is_none(), "llama's name never reaches OpenRouter");
        assert_eq!(body["repetition_penalty"], json!(1.05f32));
        assert_eq!(body["reasoning"], json!({ "enabled": false }));
        assert_eq!(body["max_tokens"], 100);
        assert!(body.get("chat_template_kwargs").is_none());
    }

    #[test]
    fn a_model_default_fills_what_translation_leaves_open() {
        let server = serve(vec![answer("ok", "stop")]);
        let profile = ModelProfile {
            supported_parameters: vec!["top_p".into(), "top_k".into(), "frequency_penalty".into()],
            reasoning: None,
            defaults: ModelDefaults { frequency_penalty: Some(0.25), top_k: Some(40), presence_penalty: Some(0.5), ..ModelDefaults::default() },
        };
        let client = ChatClient::openrouter_at(&server.base(), "k", "vendor/m").unwrap().with_profile(Some(profile));
        client.chat(&[Message::user_text("ping")], &Sampling::new(0.3, 0.9, 16)).unwrap();
        let body = body_json(&server.request(0));
        assert!(body.get("temperature").is_none(), "the model does not take temperature");
        assert_eq!(body["top_p"], json!(0.9f32));
        assert_eq!(body["top_k"], 40, "translation set no top_k: the model's own");
        assert_eq!(body["frequency_penalty"], json!(0.25));
        assert!(body.get("presence_penalty").is_none(), "published but not accepted");
        assert!(body.get("reasoning").is_none());

        let bare = serve(vec![answer("ok", "stop")]);
        let client = ChatClient::openrouter_at(&bare.base(), "k", "vendor/m").unwrap();
        client.chat(&[Message::user_text("ping")], &Sampling::new(0.3, 0.9, 16).top_k(20)).unwrap();
        let body = body_json(&bare.request(0));
        assert_eq!((body["temperature"].clone(), body["top_p"].clone()), (json!(0.3f32), json!(0.9f32)));
        assert!(body.get("top_k").is_none(), "no catalog entry: only the basics go out");
    }

    #[test]
    fn a_mandatory_thinker_gets_the_lowest_effort_and_room_for_it() {
        let server = serve(vec![Reply::json(200, r#"{"choices":[{"message":{"content":"","reasoning":"1. Hallo"},"finish_reason":"stop"}]}"#)]);
        let profile = ModelProfile {
            supported_parameters: vec![],
            reasoning: Some(ReasoningSupport { mandatory: true, supported_efforts: vec!["high".into(), "low".into()], default_effort: Some("high".into()), ..ReasoningSupport::default() }),
            defaults: ModelDefaults::default(),
        };
        let client = ChatClient::openrouter_at(&server.base(), "k", "vendor/thinker").unwrap().with_profile(Some(profile));
        assert_eq!(client.chat(&[Message::user_text("ping")], &Sampling::new(0.2, 0.95, 100)).unwrap(), "1. Hallo");
        let body = body_json(&server.request(0));
        assert_eq!(body["reasoning"], json!({ "effort": "low", "exclude": true }));
        assert_eq!(body["max_tokens"], 100 + REASONING_ROOM);
    }

    #[test]
    fn a_cut_answer_is_asked_again_with_room_then_reported() {
        let server = serve(vec![answer("1. Hal", "length"), answer("1. Hallo\n2. Welt", "stop")]);
        let client = ChatClient::openrouter_at(&server.base(), "k", "vendor/m").unwrap();
        assert_eq!(client.chat(&[Message::user_text("ping")], &Sampling::new(0.2, 0.95, 50)).unwrap(), "1. Hallo\n2. Welt");
        assert_eq!(body_json(&server.request(1))["max_tokens"], 100);

        let cut = serve(vec![answer("", "length"), answer("1.", "length")]);
        let client = ChatClient::openai_compatible(&cut.base(), "m", None).unwrap();
        let error = client.chat(&[Message::user_text("ping")], &Sampling::new(0.2, 0.95, 50)).unwrap_err();
        assert!(matches!(&error, LlmError::CutShort(text) if text.contains("finish_reason=length")), "{error}");
    }

    #[test]
    fn a_prompt_the_server_cut_is_an_error_not_a_translation() {
        let long = "word ".repeat(400);
        let server = serve(vec![Reply::json(200, r#"{"choices":[{"message":{"content":"1. x"},"finish_reason":"stop"}],"usage":{"prompt_tokens":40}}"#)]);
        let client = ChatClient::openai_compatible(&server.base(), "m", None).unwrap();
        let error = client.chat(&[Message::user_text(long)], &Sampling::new(0.2, 0.95, 50)).unwrap_err();
        assert!(matches!(&error, LlmError::PromptCut(text) if text.contains("num_ctx")), "{error}");
    }

    #[test]
    fn the_server_address_is_taken_in_any_form() {
        assert_eq!(server_base("http://127.0.0.1:11434"), "http://127.0.0.1:11434");
        assert_eq!(server_base(" http://127.0.0.1:1234/v1/ "), "http://127.0.0.1:1234");
        assert_eq!(server_base("http://gpu-box:8000/"), "http://gpu-box:8000");
    }
}
