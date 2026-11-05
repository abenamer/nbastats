//! Outbound adapter: a chat model with tool calling through OpenRouter's OpenAI-compatible
//! API. `POST {base}/chat/completions` carries the messages and the tool definitions; the
//! first choice of the answer carries text or tool calls. One request per turn, no
//! streaming, a 90-second timeout, no redirects and a 4 MB cap on the answer body (read in
//! chunks). The API key travels in the `Authorization` header only and is never logged; the
//! configuration's `Debug` output redacts it. The key can be replaced while the model runs,
//! for the key rotation. `OpenRouterKeys` is the second adapter here: the service's key
//! management (`{base}/keys`) behind a management key, which mints, lists and deletes the
//! account's keys and cannot ask a model.

use std::time::Duration;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::application::ports::{
    ChatMessage, ChatReply, ChatRequest, IssuedKey, KeyError, KeyProvisioner, KeyRecord,
    LanguageModel, ModelError, ToolCall,
};

/// Upper bound on one answer body (an answer with ten tool calls is a few KB).
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
/// Provider error messages are cut to this length before they reach the log.
const MAX_ERROR_CHARS: usize = 300;

#[derive(Clone)]
pub struct OpenRouterConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

impl std::fmt::Debug for OpenRouterConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenRouterConfig")
            .field("api_key", &"<redacted>")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .finish()
    }
}

fn short(message: String) -> String {
    message.chars().take(MAX_ERROR_CHARS).collect()
}

pub struct OpenRouterModel {
    client: reqwest::Client,
    config: OpenRouterConfig,
    /// The key in use: the configured one until the rotation installs a minted one.
    key: std::sync::RwLock<String>,
}

impl std::fmt::Debug for OpenRouterModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenRouterModel")
            .field("config", &self.config)
            .field(
                "key",
                &if self.has_key() {
                    "<redacted>"
                } else {
                    "<none>"
                },
            )
            .finish()
    }
}

impl OpenRouterModel {
    pub fn new(config: OpenRouterConfig) -> Result<Self, ModelError> {
        if config.api_key.trim().is_empty() {
            return Err(ModelError::NotConfigured(
                "OPENROUTER_API_KEY is empty".to_string(),
            ));
        }
        Self::build(config)
    }

    /// A model with no key yet: the rotation installs one. Until then a request is refused as
    /// not configured, and the log says why.
    pub fn awaiting_key(config: OpenRouterConfig) -> Result<Self, ModelError> {
        Self::build(OpenRouterConfig {
            api_key: String::new(),
            ..config
        })
    }

    fn build(config: OpenRouterConfig) -> Result<Self, ModelError> {
        if config.model.trim().is_empty() {
            return Err(ModelError::NotConfigured(
                "OPENROUTER_MODEL is empty".to_string(),
            ));
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(90))
            // The key and the conversation go to the configured host only.
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("nba-elo-simulator/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| ModelError::Transport(e.to_string()))?;
        let key = std::sync::RwLock::new(config.api_key.trim().to_string());
        Ok(Self {
            client,
            config,
            key,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }

    /// Replace the key the next requests use. A request already sent keeps the old one.
    pub fn install_key(&self, secret: String) {
        *self.key.write().unwrap_or_else(|e| e.into_inner()) = secret;
    }

    /// Whether a key is installed (the configured one or a minted one).
    pub fn has_key(&self) -> bool {
        !self
            .key
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
    }

    fn bearer(&self) -> Result<String, ModelError> {
        let key = self.key.read().unwrap_or_else(|e| e.into_inner());
        if key.is_empty() {
            return Err(ModelError::NotConfigured(
                "the question box's key has not been issued yet; see the log".to_string(),
            ));
        }
        Ok(format!("Bearer {key}"))
    }
}

/// The request body in the OpenAI chat-completions format. Tool arguments are sent back as
/// the JSON text the API expects.
pub fn request_body(model: &str, request: &ChatRequest<'_>) -> Value {
    let messages: Vec<Value> = request.messages.iter().map(message_json).collect();
    let mut body = json!({
        "model": model,
        "messages": messages,
        "max_tokens": request.max_tokens,
        "temperature": 0,
    });
    if !request.tools.is_empty() {
        body["tools"] = request
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": { "name": t.name, "description": t.description, "parameters": t.parameters }
                })
            })
            .collect();
        body["tool_choice"] = json!("auto");
    }
    body
}

fn message_json(m: &ChatMessage) -> Value {
    match m {
        ChatMessage::System(s) => json!({ "role": "system", "content": s }),
        ChatMessage::User(s) => json!({ "role": "user", "content": s }),
        ChatMessage::Assistant {
            content,
            tool_calls,
        } => {
            let mut v = json!({ "role": "assistant", "content": content });
            if !tool_calls.is_empty() {
                v["tool_calls"] = tool_calls
                    .iter()
                    .map(|c| {
                        json!({
                            "id": c.id,
                            "type": "function",
                            "function": { "name": c.name, "arguments": c.arguments.to_string() }
                        })
                    })
                    .collect();
            }
            v
        }
        ChatMessage::Tool { call_id, content } => {
            json!({ "role": "tool", "tool_call_id": call_id, "content": content })
        }
    }
}

#[derive(Deserialize)]
struct Completion {
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
    #[serde(default)]
    error: Option<ApiErrorBody>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    content: Option<Value>,
    #[serde(default)]
    tool_calls: Vec<RawToolCall>,
}

#[derive(Deserialize)]
struct RawToolCall {
    #[serde(default)]
    id: String,
    function: RawFunction,
}

#[derive(Deserialize)]
struct RawFunction {
    name: String,
    /// The API sends the arguments as a JSON string; some providers send an object.
    #[serde(default)]
    arguments: Value,
}

#[derive(Deserialize, Default)]
struct Usage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
}

#[derive(Deserialize)]
struct ApiErrorBody {
    #[serde(default)]
    message: String,
}

/// Parse a chat completion; the first choice is the answer.
pub fn parse_reply(bytes: &[u8]) -> Result<ChatReply, ModelError> {
    let c: Completion = serde_json::from_slice(bytes)
        .map_err(|e| ModelError::InvalidResponse(format!("not a chat completion: {e}")))?;
    if let Some(e) = c.error {
        return Err(ModelError::Rejected {
            status: 200,
            message: short(e.message),
        });
    }
    let choice = c
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| ModelError::InvalidResponse("no choices in the answer".to_string()))?;
    let content = match choice.message.content {
        Some(Value::String(s)) if !s.trim().is_empty() => Some(s),
        Some(Value::Array(parts)) => {
            let text: Vec<&str> = parts
                .iter()
                .filter_map(|p| p.get("text").and_then(Value::as_str))
                .collect();
            let joined = text.join("");
            (!joined.trim().is_empty()).then_some(joined)
        }
        _ => None,
    };
    let tool_calls = choice
        .message
        .tool_calls
        .into_iter()
        .enumerate()
        .map(|(i, t)| {
            let arguments = match t.function.arguments {
                Value::String(s) if s.trim().is_empty() => json!({}),
                Value::String(s) => match serde_json::from_str::<Value>(&s) {
                    Ok(v) => v,
                    Err(_) => json!({ "_raw": s }),
                },
                Value::Null => json!({}),
                v => v,
            };
            ToolCall {
                id: if t.id.is_empty() {
                    format!("missing_id_{i}")
                } else {
                    t.id
                },
                name: t.function.name,
                arguments,
            }
        })
        .collect();
    let usage = c.usage.unwrap_or_default();
    Ok(ChatReply {
        content,
        tool_calls,
        input_tokens: usage.prompt_tokens,
        output_tokens: usage.completion_tokens,
    })
}

impl LanguageModel for OpenRouterModel {
    fn model_name(&self) -> &str {
        &self.config.model
    }

    async fn chat(&self, request: ChatRequest<'_>) -> Result<ChatReply, ModelError> {
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let bearer = self.bearer()?;
        let body = serde_json::to_vec(&request_body(&self.config.model, &request))
            .map_err(|e| ModelError::Transport(e.to_string()))?;
        let resp = self
            .client
            .post(&url)
            .header(AUTHORIZATION, bearer)
            .header(CONTENT_TYPE, "application/json")
            .header("X-Title", "NBA Elo Simulator")
            .body(body)
            .send()
            .await
            .map_err(|e| ModelError::Transport(e.to_string()))?;
        let status = resp.status();
        if let Some(len) = resp.content_length() {
            if len as usize > MAX_BODY_BYTES {
                return Err(ModelError::InvalidResponse(format!(
                    "answer of {len} bytes"
                )));
            }
        }
        // Chunked answers carry no length: stop reading at the cap.
        let mut resp = resp;
        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| ModelError::Transport(e.to_string()))?
        {
            bytes.extend_from_slice(&chunk);
            if bytes.len() > MAX_BODY_BYTES {
                return Err(ModelError::InvalidResponse(format!(
                    "answer longer than {MAX_BODY_BYTES} bytes"
                )));
            }
        }
        if !status.is_success() {
            let message = serde_json::from_slice::<Completion>(&bytes)
                .ok()
                .and_then(|c| c.error)
                .map(|e| short(e.message))
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| short(String::from_utf8_lossy(&bytes).into_owned()));
            return Err(ModelError::Rejected {
                status: status.as_u16(),
                message,
            });
        }
        parse_reply(&bytes)
    }
}

// ---------------------------------------------------------------------------------------
// Key management
// ---------------------------------------------------------------------------------------

/// Upper bound on a key listing page (a hundred keys are a few tens of KB).
const MAX_KEYS_BODY_BYTES: usize = 1024 * 1024;
/// Pages of the listing read at most (the account's keys are a handful).
const MAX_KEY_PAGES: usize = 10;

/// The service's key management behind a management key. `POST {base}/keys` mints a key and
/// returns its secret once; `GET {base}/keys?include_disabled=true` lists the account's keys
/// by hash and name; `DELETE {base}/keys/{hash}` deletes one. The management key travels in
/// the `Authorization` header only and is never logged.
pub struct OpenRouterKeys {
    client: reqwest::Client,
    management_key: String,
    base_url: String,
}

impl std::fmt::Debug for OpenRouterKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenRouterKeys")
            .field("management_key", &"<redacted>")
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl OpenRouterKeys {
    pub fn new(management_key: String, base_url: String) -> Result<Self, KeyError> {
        let management_key = management_key.trim().to_string();
        if management_key.is_empty() {
            return Err(KeyError::InvalidResponse(
                "OPENROUTER_MANAGEMENT_KEY is empty".to_string(),
            ));
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("nba-elo-simulator/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| KeyError::Transport(e.to_string()))?;
        Ok(Self {
            client,
            management_key,
            base_url,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn url(&self, path: &str) -> String {
        format!("{}/keys{path}", self.base_url.trim_end_matches('/'))
    }

    /// Send a request and return the body of a successful answer, or the service's error.
    async fn send(&self, request: reqwest::RequestBuilder) -> Result<Vec<u8>, KeyError> {
        let resp = request
            .header(AUTHORIZATION, format!("Bearer {}", self.management_key))
            .send()
            .await
            .map_err(|e| KeyError::Transport(e.without_url().to_string()))?;
        let status = resp.status();
        let mut resp = resp;
        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| KeyError::Transport(e.without_url().to_string()))?
        {
            bytes.extend_from_slice(&chunk);
            if bytes.len() > MAX_KEYS_BODY_BYTES {
                return Err(KeyError::InvalidResponse(format!(
                    "answer longer than {MAX_KEYS_BODY_BYTES} bytes"
                )));
            }
        }
        if !status.is_success() {
            let message = serde_json::from_slice::<Completion>(&bytes)
                .ok()
                .and_then(|c| c.error)
                .map(|e| short(e.message))
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| short(String::from_utf8_lossy(&bytes).into_owned()));
            return Err(KeyError::Rejected {
                status: status.as_u16(),
                message,
            });
        }
        Ok(bytes)
    }
}

#[derive(Deserialize)]
struct KeyData {
    hash: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    disabled: bool,
}

#[derive(Deserialize)]
struct CreatedKey {
    data: KeyData,
    key: String,
}

#[derive(Deserialize)]
struct KeyList {
    data: Vec<KeyData>,
}

/// The minted key from the creation answer: the secret and the hash.
pub fn parse_created(bytes: &[u8]) -> Result<IssuedKey, KeyError> {
    let c: CreatedKey = serde_json::from_slice(bytes)
        .map_err(|e| KeyError::InvalidResponse(format!("creation answer: {e}")))?;
    if c.key.trim().is_empty() || c.data.hash.trim().is_empty() {
        return Err(KeyError::InvalidResponse(
            "creation answer without a key or a hash".to_string(),
        ));
    }
    Ok(IssuedKey {
        secret: c.key.trim().to_string(),
        hash: c.data.hash,
    })
}

/// The account's keys from the listing answer.
pub fn parse_list(bytes: &[u8]) -> Result<Vec<KeyRecord>, KeyError> {
    let l: KeyList = serde_json::from_slice(bytes)
        .map_err(|e| KeyError::InvalidResponse(format!("listing answer: {e}")))?;
    Ok(l.data
        .into_iter()
        .filter(|k| !k.hash.trim().is_empty())
        .map(|k| KeyRecord {
            hash: k.hash,
            name: k.name,
            disabled: k.disabled,
        })
        .collect())
}

impl KeyProvisioner for OpenRouterKeys {
    async fn create(&self, name: &str, limit: Option<f64>) -> Result<IssuedKey, KeyError> {
        let mut body = json!({ "name": name });
        if let Some(limit) = limit {
            body["limit"] = json!(limit);
        }
        let body = serde_json::to_vec(&body).map_err(|e| KeyError::Transport(e.to_string()))?;
        let bytes = self
            .send(
                self.client
                    .post(self.url(""))
                    .header(CONTENT_TYPE, "application/json")
                    .body(body),
            )
            .await?;
        parse_created(&bytes)
    }

    async fn list(&self) -> Result<Vec<KeyRecord>, KeyError> {
        // The listing is paged by `offset`; pages are read until one is empty or brings
        // nothing new (a service that ignores the offset answers the same page again), up
        // to the cap.
        let mut all: Vec<KeyRecord> = Vec::new();
        for _ in 0..MAX_KEY_PAGES {
            let bytes = self
                .send(self.client.get(self.url("")).query(&[
                    ("include_disabled", "true".to_string()),
                    ("offset", all.len().to_string()),
                ]))
                .await?;
            let page = parse_list(&bytes)?;
            let before = all.len();
            for k in page {
                if !all.iter().any(|a| a.hash == k.hash) {
                    all.push(k);
                }
            }
            if all.len() == before {
                break;
            }
        }
        Ok(all)
    }

    async fn delete(&self, hash: &str) -> Result<(), KeyError> {
        // The hash names one path segment: letters, digits, `-` and `_` only.
        if hash.is_empty()
            || !hash
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(KeyError::InvalidResponse(format!(
                "key hash not usable in a path: {}",
                short(hash.to_string())
            )));
        }
        self.send(self.client.delete(self.url(&format!("/{hash}"))))
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::ToolSpec;

    #[test]
    fn a_minted_key_and_a_listing_are_read_and_bad_answers_refused() {
        let k = parse_created(
            br#"{"data":{"hash":"abc123","name":"nba-elo-simulator 2026-11-05T07:48Z","label":"sk-or-v1-...9f","disabled":false,"limit":5,"usage":0},"key":"sk-or-v1-secret"}"#,
        )
        .unwrap();
        assert_eq!(k.secret, "sk-or-v1-secret");
        assert_eq!(k.hash, "abc123");
        assert!(matches!(
            parse_created(br#"{"data":{"hash":"abc123"}}"#),
            Err(KeyError::InvalidResponse(_))
        ));
        assert!(matches!(
            parse_created(br#"{"data":{"hash":"abc123"},"key":" "}"#),
            Err(KeyError::InvalidResponse(_))
        ));
        let l = parse_list(
            br#"{"data":[{"hash":"h1","name":"nba-elo-simulator 2026-11-04T07:48Z","disabled":false},{"hash":"h2","name":"Laptop","disabled":true},{"hash":"","name":"odd"}]}"#,
        )
        .unwrap();
        assert_eq!(
            l,
            vec![
                KeyRecord {
                    hash: "h1".into(),
                    name: "nba-elo-simulator 2026-11-04T07:48Z".into(),
                    disabled: false
                },
                KeyRecord {
                    hash: "h2".into(),
                    name: "Laptop".into(),
                    disabled: true
                }
            ]
        );
        assert!(matches!(
            parse_list(b"<html>"),
            Err(KeyError::InvalidResponse(_))
        ));
    }

    #[test]
    fn the_management_key_is_required_and_redacted() {
        assert!(matches!(
            OpenRouterKeys::new(" ".into(), "https://openrouter.ai/api/v1".into()),
            Err(KeyError::InvalidResponse(_))
        ));
        let k = OpenRouterKeys::new(
            "sk-or-mgmt-secret".into(),
            "https://openrouter.ai/api/v1".into(),
        )
        .unwrap();
        let shown = format!("{k:?}");
        assert!(shown.contains("<redacted>") && !shown.contains("secret"));
        assert_eq!(k.url("/h1"), "https://openrouter.ai/api/v1/keys/h1");
    }

    #[tokio::test]
    async fn a_model_awaiting_its_key_refuses_requests_until_one_is_installed() {
        let m = OpenRouterModel::awaiting_key(OpenRouterConfig {
            api_key: "ignored".into(),
            base_url: "http://127.0.0.1:9".into(),
            model: "m".into(),
        })
        .unwrap();
        assert!(!m.has_key());
        assert!(matches!(m.bearer(), Err(ModelError::NotConfigured(_))));
        m.install_key("sk-or-v1-minted".into());
        assert!(m.has_key());
        assert_eq!(m.bearer().unwrap(), "Bearer sk-or-v1-minted");
        let shown = format!("{m:?}");
        assert!(shown.contains("<redacted>") && !shown.contains("minted"));
    }

    #[test]
    fn request_carries_messages_tools_and_tool_arguments_as_text() {
        let messages = vec![
            ChatMessage::System("be brief".into()),
            ChatMessage::User("who won in 2016?".into()),
            ChatMessage::Assistant {
                content: None,
                tool_calls: vec![ToolCall {
                    id: "c1".into(),
                    name: "champions".into(),
                    arguments: json!({ "from": 2016, "to": 2016 }),
                }],
            },
            ChatMessage::Tool {
                call_id: "c1".into(),
                content: "[]".into(),
            },
        ];
        let tools = vec![ToolSpec {
            name: "champions",
            description: "titles",
            parameters: json!({ "type": "object", "properties": {} }),
        }];
        let body = request_body(
            "openai/gpt-4o-mini",
            &ChatRequest {
                messages: &messages,
                tools: &tools,
                max_tokens: 100,
            },
        );
        assert_eq!(body["model"], "openai/gpt-4o-mini");
        assert_eq!(body["messages"].as_array().unwrap().len(), 4);
        assert_eq!(
            body["messages"][2]["tool_calls"][0]["function"]["name"],
            "champions"
        );
        assert_eq!(
            body["messages"][2]["tool_calls"][0]["function"]["arguments"],
            r#"{"from":2016,"to":2016}"#
        );
        assert_eq!(body["messages"][3]["tool_call_id"], "c1");
        assert_eq!(body["tools"][0]["function"]["name"], "champions");
        assert_eq!(body["tool_choice"], "auto");
        // Without tools the request carries no tool fields (the final answer turn).
        let bare = request_body(
            "m",
            &ChatRequest {
                messages: &messages,
                tools: &[],
                max_tokens: 10,
            },
        );
        assert!(bare.get("tools").is_none() && bare.get("tool_choice").is_none());
    }

    #[test]
    fn replies_with_tool_calls_text_and_errors_are_parsed() {
        let calls = br#"{"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[
            {"id":"call_1","type":"function","function":{"name":"season_standings","arguments":"{\"season\": 2016}"}},
            {"type":"function","function":{"name":"list_seasons","arguments":""}}]}}],
            "usage":{"prompt_tokens":120,"completion_tokens":30}}"#;
        let r = parse_reply(calls).unwrap();
        assert_eq!(r.content, None);
        assert_eq!(r.tool_calls.len(), 2);
        assert_eq!(r.tool_calls[0].arguments, json!({ "season": 2016 }));
        assert_eq!(r.tool_calls[1].id, "missing_id_1");
        assert_eq!(r.tool_calls[1].arguments, json!({}));
        assert_eq!((r.input_tokens, r.output_tokens), (120, 30));

        let text = br#"{"choices":[{"message":{"role":"assistant","content":"Cleveland won."}}]}"#;
        let r = parse_reply(text).unwrap();
        assert_eq!(r.content.as_deref(), Some("Cleveland won."));
        assert!(r.tool_calls.is_empty());

        let parts = br#"{"choices":[{"message":{"content":[{"type":"text","text":"A"},{"type":"text","text":"B"}]}}]}"#;
        assert_eq!(parse_reply(parts).unwrap().content.as_deref(), Some("AB"));

        let error = br#"{"error":{"message":"insufficient credits","code":402}}"#;
        assert!(matches!(
            parse_reply(error),
            Err(ModelError::Rejected { status: 200, message }) if message == "insufficient credits"
        ));
        assert!(matches!(
            parse_reply(b"<html>"),
            Err(ModelError::InvalidResponse(_))
        ));
        assert!(matches!(
            parse_reply(br#"{"choices":[]}"#),
            Err(ModelError::InvalidResponse(_))
        ));
    }

    #[test]
    fn debug_output_redacts_the_key() {
        let c = OpenRouterConfig {
            api_key: "sk-or-secret".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            model: "m".into(),
        };
        let shown = format!("{c:?}");
        assert!(shown.contains("<redacted>") && !shown.contains("secret"));
        assert_eq!(short("x".repeat(1000)).chars().count(), MAX_ERROR_CHARS);
    }

    #[test]
    fn an_empty_key_is_a_configuration_error() {
        assert!(matches!(
            OpenRouterModel::new(OpenRouterConfig {
                api_key: " ".into(),
                base_url: "https://openrouter.ai/api/v1".into(),
                model: "m".into(),
            }),
            Err(ModelError::NotConfigured(_))
        ));
    }
}
