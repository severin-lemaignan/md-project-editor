use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde_json::{json, Value};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

const DEFAULT_LOCAL_BASE_URL: &str = "http://localhost:11434/v1";
const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
#[derive(Clone, Debug)]
pub enum EditMode {
    InsertAtCursor,
    ReplaceSelection,
}

#[derive(Clone, Debug)]
pub struct EditRequest {
    pub instruction: String,
    pub document_name: Option<String>,
    pub mode: EditMode,
    pub selected_text: Option<String>,
    pub context_before: String,
    pub context_after: String,
}

#[derive(Clone, Debug)]
pub struct EditResponse {
    pub text: String,
    pub summary: Option<String>,
}

pub type SharedAiProvider = Arc<dyn AiProvider>;

pub trait AiProvider: Send + Sync {
    fn request_edit(&self, request: &EditRequest) -> Result<EditResponse, String>;
}

#[derive(Clone)]
pub struct ProviderAvailability {
    provider: Option<SharedAiProvider>,
    error: Option<String>,
}

impl ProviderAvailability {
    pub fn from_env() -> Self {
        match OpenAiCompatProvider::from_env() {
            Ok(provider) => Self {
                provider: Some(Arc::new(provider)),
                error: None,
            },
            Err(err) => Self {
                provider: None,
                error: Some(err),
            },
        }
    }

    pub fn provider(&self) -> Option<SharedAiProvider> {
        self.provider.clone()
    }

    pub fn error_message(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

pub fn request_edit_in_background(
    provider: SharedAiProvider,
    request: EditRequest,
) -> Receiver<Result<EditResponse, String>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = provider.request_edit(&request);
        let _ = sender.send(result);
    });
    receiver
}

#[derive(Clone)]
pub struct OpenAiCompatProvider {
    client: Client,
    config: OpenAiCompatConfig,
}

#[derive(Clone, Debug)]
struct OpenAiCompatConfig {
    base_url: String,
    api_key: Option<String>,
    model: String,
}

impl OpenAiCompatProvider {
    pub fn from_env() -> Result<Self, String> {
        let config = OpenAiCompatConfig::from_env()?;
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        if let Some(key) = &config.api_key {
            let bearer = format!("Bearer {key}");
            let value = HeaderValue::from_str(&bearer)
                .map_err(|err| format!("Invalid API key header: {err}"))?;
            headers.insert(AUTHORIZATION, value);
        }

        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|err| format!("Failed to build AI client: {err}"))?;

        Ok(Self { client, config })
    }
}

impl AiProvider for OpenAiCompatProvider {
    fn request_edit(&self, request: &EditRequest) -> Result<EditResponse, String> {
        let endpoint = format!("{}/chat/completions", self.config.base_url.trim_end_matches('/'));
        let system_prompt = system_prompt();
        let user_prompt = build_user_prompt(request);
        let body = json!({
            "model": self.config.model,
            "temperature": 0.2,
            "messages": [
                {
                    "role": "system",
                    "content": system_prompt,
                },
                {
                    "role": "user",
                    "content": user_prompt,
                }
            ]
        });

        let response = self
            .client
            .post(endpoint)
            .json(&body)
            .send()
            .map_err(|err| format!("AI request failed: {err}"))?;

        let status = response.status();
        let value: Value = response
            .json()
            .map_err(|err| format!("Failed to parse AI response: {err}"))?;

        if !status.is_success() {
            return Err(extract_api_error(&value).unwrap_or_else(|| {
                format!("AI request failed with status {status}")
            }));
        }

        let content = extract_message_content(&value)?;
        parse_edit_response(&content)
    }
}

impl OpenAiCompatConfig {
    fn from_env() -> Result<Self, String> {
        let api_key = env_var("AGENTIC_MD_AI_API_KEY").or_else(|| env_var("OPENAI_API_KEY"));
        let explicit_base_url = env_var("AGENTIC_MD_AI_BASE_URL").or_else(|| env_var("OPENAI_BASE_URL"));
        let model = env_var("AGENTIC_MD_AI_MODEL")
            .or_else(|| env_var("OPENAI_MODEL"))
            .ok_or_else(|| {
                "AI is not configured. Set AGENTIC_MD_AI_MODEL (and optionally AGENTIC_MD_AI_BASE_URL / AGENTIC_MD_AI_API_KEY).".to_string()
            })?;

        let base_url = explicit_base_url.unwrap_or_else(|| {
            if api_key.is_some() {
                DEFAULT_OPENAI_BASE_URL.to_string()
            } else {
                DEFAULT_LOCAL_BASE_URL.to_string()
            }
        });

        Ok(Self {
            base_url,
            api_key,
            model,
        })
    }
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn system_prompt() -> &'static str {
    "You are assisting with academic writing in Pandoc Markdown. Preserve citations, footnotes, LaTeX math, and Pandoc-specific syntax. Return only JSON in the form {\"text\":\"...\",\"summary\":\"...\"}. The text field must contain only the inserted or replacement text, not the surrounding document."
}

fn build_user_prompt(request: &EditRequest) -> String {
    let doc_name = request
        .document_name
        .as_deref()
        .unwrap_or("[untitled]");
    let before = if request.context_before.is_empty() {
        "[start of document]"
    } else {
        request.context_before.as_str()
    };
    let after = if request.context_after.is_empty() {
        "[end of document]"
    } else {
        request.context_after.as_str()
    };

    match request.mode {
        EditMode::InsertAtCursor => format!(
            "Document: {doc_name}\nTask: Insert new text at the cursor.\nInstruction: {}\n\nContext before cursor:\n<<<BEFORE>>>\n{before}\n<<<END BEFORE>>>\n\nContext after cursor:\n<<<AFTER>>>\n{after}\n<<<END AFTER>>>\n\nReturn JSON only. The text field should contain only the new text to insert at the cursor.",
            request.instruction.trim()
        ),
        EditMode::ReplaceSelection => format!(
            "Document: {doc_name}\nTask: Rewrite the selected text.\nInstruction: {}\n\nSelected text:\n<<<SELECTION>>>\n{}\n<<<END SELECTION>>>\n\nContext before selection:\n<<<BEFORE>>>\n{before}\n<<<END BEFORE>>>\n\nContext after selection:\n<<<AFTER>>>\n{after}\n<<<END AFTER>>>\n\nReturn JSON only. The text field should contain only the replacement text for the selected block.",
            request.instruction.trim(),
            request.selected_text.as_deref().unwrap_or_default(),
        ),
    }
}

fn extract_api_error(value: &Value) -> Option<String> {
    value
        .get("error")
        .and_then(|error| {
            error
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| error.as_str())
        })
        .map(|message| message.to_string())
}

fn extract_message_content(value: &Value) -> Result<String, String> {
    let content = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .ok_or_else(|| "AI response did not contain a message.".to_string())?;

    if let Some(text) = content.as_str() {
        return Ok(text.to_string());
    }

    if let Some(parts) = content.as_array() {
        let mut text = String::new();
        for part in parts {
            if part.get("type").and_then(Value::as_str) == Some("text") {
                if let Some(value) = part.get("text").and_then(Value::as_str) {
                    text.push_str(value);
                }
            }
        }
        if !text.trim().is_empty() {
            return Ok(text);
        }
    }

    Err("AI response content format is not supported.".to_string())
}

fn parse_edit_response(content: &str) -> Result<EditResponse, String> {
    let stripped = strip_code_fences(content).trim().to_string();

    if let Some(json_payload) = extract_json_object(&stripped) {
        if let Ok(value) = serde_json::from_str::<Value>(&json_payload) {
            if let Some(text) = value.get("text").and_then(Value::as_str) {
                return Ok(EditResponse {
                    text: text.to_string(),
                    summary: value
                        .get("summary")
                        .and_then(Value::as_str)
                        .map(|summary| summary.to_string()),
                });
            }
        }
    }

    if stripped.is_empty() {
        Err("AI returned an empty response.".to_string())
    } else {
        Ok(EditResponse {
            text: stripped,
            summary: None,
        })
    }
}

fn strip_code_fences(content: &str) -> String {
    let trimmed = content.trim();
    if let Some(rest) = trimmed.strip_prefix("```") {
        let without_language = rest
            .split_once('\n')
            .map(|(_, tail)| tail)
            .unwrap_or_default();
        return without_language
            .strip_suffix("```")
            .unwrap_or(without_language)
            .trim()
            .to_string();
    }
    trimmed.to_string()
}

fn extract_json_object(content: &str) -> Option<String> {
    let start = content.find('{')?;
    let end = content.rfind('}')?;
    (start <= end).then(|| content[start..=end].to_string())
}
