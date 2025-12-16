use std::env;

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use super::{ChatMessage, ChatReply, LLMClient, Role};

pub struct OpenAIClient {
    api_key: String,
    model: String,
    base_url: String,
    client: Client,
    system_prompt: String,
}

impl OpenAIClient {
    pub fn from_env() -> Result<Self> {
        let api_key =
            env::var("OPENAI_API_KEY").context("OPENAI_API_KEY is required for OpenAI provider")?;
        let model = env::var("OPENAI_MODEL").unwrap_or("gpt-4o-mini".to_string());
        let base_url = env::var("OPENAI_BASE_URL")
            .unwrap_or("https://api.openai.com/v1".to_string());
        let client = Client::builder().build()?;

        let system_prompt =
            "You are a focused shell copilot. Always respond ONLY with a JSON object: \
            {\"command\": \"<shell command>\", \"answer\": \"brief human-readable note\"}. \
            Do not add code fences or extra text. Prefer safe defaults; if unsure ask via answer."
                .to_string();

        Ok(Self {
            api_key,
            model,
            base_url,
            client,
            system_prompt,
        })
    }
}

#[derive(Serialize)]
struct OaiRequest<'a> {
    model: &'a str,
    messages: Vec<serde_json::Value>,
    #[serde(rename = "response_format")]
    response_format: ResponseFormat<'a>,
}

#[derive(Deserialize)]
struct OaiResponse {
    choices: Vec<OaiChoice>,
}

#[derive(Deserialize)]
struct OaiChoice {
    message: OaiRespMessage,
}

#[derive(Deserialize)]
struct OaiRespMessage {
    content: String,
}

#[derive(Serialize)]
struct ResponseFormat<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
}

#[derive(Deserialize)]
struct JsonPayload {
    command: Option<String>,
    answer: Option<String>,
    note: Option<String>,
    explanation: Option<String>,
    message: Option<String>,
}

impl LLMClient for OpenAIClient {
    fn chat(&self, history: &[ChatMessage], user_input: &str) -> Result<ChatReply> {
        let mut payload: Vec<serde_json::Value> = Vec::with_capacity(history.len() + 2);
        payload.push(serde_json::json!({ "role": "system", "content": self.system_prompt }));
        for m in history {
            let role = match m.role {
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            payload.push(serde_json::json!({ "role": role, "content": m.content }));
        }
        payload.push(serde_json::json!({"role": "user", "content": user_input}));

        let req = OaiRequest {
            model: &self.model,
            messages: payload,
            response_format: ResponseFormat {
                kind: "json_object",
            },
        };

        let endpoint = format!("{}/chat/completions", self.base_url);
        let resp: OaiResponse = self
            .client
            .post(&endpoint)
            .bearer_auth(&self.api_key)
            .json(&req)
            .send()
            .context("failed to call OpenAI")?
            .error_for_status()
            .context("OpenAI returned error status")?
            .json()
            .context("failed to parse OpenAI response")?;

        let text = resp
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_else(|| "No response".to_string());
        let suggested_command;
        let display_text;
        match serde_json::from_str::<JsonPayload>(&text) {
            Ok(json) => {
                suggested_command = json.command.clone();
                display_text = json
                    .answer
                    .or(json.note)
                    .or(json.explanation)
                    .or(json.message)
                    .unwrap_or_else(|| "".to_string());
            }
            Err(_) => {
                suggested_command = None;
                display_text = text.clone();
            }
        }

        Ok(ChatReply {
            text: if display_text.is_empty() {
                text
            } else {
                display_text
            },
            suggested_command,
        })
    }
}
