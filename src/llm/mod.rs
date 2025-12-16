pub mod openai;

use anyhow::Result;

#[derive(Clone, Copy, Debug)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

#[derive(Clone, Debug)]
pub struct ChatReply {
    pub text: String,
    pub suggested_command: Option<String>,
    pub reasoning: Option<String>,
}

pub trait LLMClient: Send + Sync {
    fn chat(
        &self,
        history: &[ChatMessage],
        user_input: &str,
        on_reasoning: &mut dyn FnMut(&str),
    ) -> Result<ChatReply>;
}
