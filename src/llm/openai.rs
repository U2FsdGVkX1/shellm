use std::io::{BufRead, BufReader};

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use super::{ChatMessage, ChatReply, LLMClient, Role};
use crate::i18n::{Language, MessageKey, t};

pub struct OpenAIClient {
    api_key: String,
    model: String,
    base_url: String,
    client: Client,
    system_prompt: String,
    lang: Language,
}

impl OpenAIClient {
    pub fn new(
        api_key: String,
        model: String,
        base_url: String,
        system_prompt: String,
        lang: Language,
    ) -> Result<Self> {
        let client = Client::builder().build()?;
        Ok(Self {
            api_key,
            model,
            base_url,
            client,
            system_prompt,
            lang,
        })
    }
}

#[derive(Serialize)]
struct OaiRequest<'a> {
    model: &'a str,
    messages: Vec<serde_json::Value>,
    #[serde(rename = "response_format")]
    response_format: ResponseFormat<'a>,
    stream: bool,
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

// Data structures for streaming responses
#[derive(Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Deserialize)]
struct StreamDelta {
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

fn extract_json(content: &str) -> &str {
    let trimmed = content.trim();
    if let Some(start) = trimmed.find("```json") {
        let json_start = start + 7;
        if let Some(end) = trimmed[json_start..].find("```") {
            return trimmed[json_start..json_start + end].trim();
        }
    }
    if let Some(start) = trimmed.find("```") {
        let json_start = start + 3;
        if let Some(end) = trimmed[json_start..].find("```") {
            return trimmed[json_start..json_start + end].trim();
        }
    }
    trimmed
}

impl LLMClient for OpenAIClient {
    fn chat(
        &self,
        history: &[ChatMessage],
        user_input: &str,
        on_reasoning: &mut dyn FnMut(&str),
    ) -> Result<ChatReply> {
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
            stream: true,
        };

        let endpoint = format!("{}/chat/completions", self.base_url);
        let resp = self
            .client
            .post(&endpoint)
            .bearer_auth(&self.api_key)
            .json(&req)
            .send()
            .context("failed to call OpenAI")?
            .error_for_status()
            .context("OpenAI returned error status")?;

        // Use BufReader to read streaming responses line by line
        let reader = BufReader::new(resp);
        let mut accumulated_content = String::new();
        let mut accumulated_reasoning = String::new();

        for line in reader.lines() {
            let line = line.context("failed to read line from stream")?;
            
            // SSE format: data lines start with "data: "
            if let Some(data) = line.strip_prefix("data: ") {
                // Stream end marker
                if data == "[DONE]" {
                    break;
                }

                // Parse JSON chunk
                if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                    if let Some(choice) = chunk.choices.first() {
                        // Handle reasoning content
                        if let Some(reasoning) = &choice.delta.reasoning_content {
                            accumulated_reasoning.push_str(reasoning);
                            on_reasoning(reasoning);
                        }
                        
                        // Accumulate standard content
                        if let Some(content) = &choice.delta.content {
                            accumulated_content.push_str(content);
                        }
                    }
                }
            }
        }

        let suggested_command;
        let display_text;

        let json_str = extract_json(&accumulated_content);
        match serde_json::from_str::<JsonPayload>(json_str) {
            Ok(json) => {
                suggested_command = json.command.clone();
                display_text = json
                    .answer
                    .or(json.note)
                    .or(json.explanation)
                    .or(json.message)
                    .unwrap_or_default();
            }
            Err(e) => {
                suggested_command = None;
                let error_prefix = t(&self.lang, MessageKey::JsonParseError);
                display_text = format!("{}{}]\n{}", error_prefix, e, accumulated_content);
            }
        }

        Ok(ChatReply {
            text: if display_text.is_empty() {
                accumulated_content
            } else {
                display_text
            },
            suggested_command,
            reasoning: if accumulated_reasoning.is_empty() {
                None
            } else {
                Some(accumulated_reasoning)
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_with_json_fence() {
        let input = r#"```json
{"command": "ls -la", "answer": "list files"}
```"#;
        let result = extract_json(input);
        assert_eq!(result, r#"{"command": "ls -la", "answer": "list files"}"#);
    }

    #[test]
    fn test_extract_json_with_generic_fence() {
        let input = r#"```
{"command": "pwd", "answer": "print working directory"}
```"#;
        let result = extract_json(input);
        assert_eq!(result, r#"{"command": "pwd", "answer": "print working directory"}"#);
    }

    #[test]
    fn test_extract_json_plain() {
        let input = r#"{"command": "echo hello", "answer": "prints hello"}"#;
        let result = extract_json(input);
        assert_eq!(result, r#"{"command": "echo hello", "answer": "prints hello"}"#);
    }

    #[test]
    fn test_extract_json_with_whitespace() {
        let input = r#"
```json
{
    "command": "du -sh ~",
    "answer": "查看主目录占用空间"
}
```
"#;
        let result = extract_json(input);
        assert!(result.contains(r#""command": "du -sh ~""#));
    }

    #[test]
    fn test_extract_json_with_text_before_fence() {
        let input = r#"Here is your command:
```json
{"command": "cat /etc/passwd", "answer": "view passwd file"}
```"#;
        let result = extract_json(input);
        assert_eq!(result, r#"{"command": "cat /etc/passwd", "answer": "view passwd file"}"#);
    }

    #[test]
    fn test_extract_json_unclosed_fence() {
        let input = r#"```json
{"command": "ls"}"#;
        let result = extract_json(input);
        assert_eq!(result, input.trim());
    }
}
