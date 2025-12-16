#[derive(Debug, Clone, Copy, Default)]
pub enum Language {
    #[default]
    En,
    Zh,
}

impl Language {
    pub fn from_str(s: &str) -> Self {
        let s = s.to_lowercase();
        if s.starts_with("zh") {
            Language::Zh
        } else {
            Language::En
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MessageKey {
    WelcomeMessage,
    PromptUser,
    PromptAssistant,
    PromptCandidate,
    ThinkingProcess,
    HintToggleReasoning,
    ReasoningStart,
    ReasoningEnd,
    ReasoningTruncated,
}

pub fn t(lang: &Language, key: MessageKey) -> &'static str {
    match (lang, key) {
        // Welcome message
        (Language::En, MessageKey::WelcomeMessage) => {
            "[LLM chat] Type your question. Ctrl+L accepts the command. Ctrl+C exits. Ctrl+R toggles reasoning."
        }
        (Language::Zh, MessageKey::WelcomeMessage) => {
            "[LLM chat] 输入您的问题。Ctrl+L 接受命令，Ctrl+C 退出，Ctrl+R 展开/折叠思维链。"
        }

        // User input prompt
        (Language::En, MessageKey::PromptUser) => "you> ",
        (Language::Zh, MessageKey::PromptUser) => "你> ",

        // AI response prompt
        (Language::En, MessageKey::PromptAssistant) => "assistant> ",
        (Language::Zh, MessageKey::PromptAssistant) => "助手> ",

        // Candidate command prompt
        (Language::En, MessageKey::PromptCandidate) => "candidate: ",
        (Language::Zh, MessageKey::PromptCandidate) => "候选命令: ",

        // “Thinking” indicator
        (Language::En, MessageKey::ThinkingProcess) => "[Thinking] ",
        (Language::Zh, MessageKey::ThinkingProcess) => "[思考中] ",

        // Hint for expanding/collapsing reasoning
        (Language::En, MessageKey::HintToggleReasoning) => "(Ctrl+R to expand/collapse reasoning)",
        (Language::Zh, MessageKey::HintToggleReasoning) => "(Ctrl+R 展开/折叠思维链)",

        // Reasoning section start marker
        (Language::En, MessageKey::ReasoningStart) => "--- Reasoning ---",
        (Language::Zh, MessageKey::ReasoningStart) => "--- 思维链 ---",

        // Reasoning section end marker
        (Language::En, MessageKey::ReasoningEnd) => "--- End ---",
        (Language::Zh, MessageKey::ReasoningEnd) => "--- 结束 ---",

        // Reasoning content truncated marker
        (Language::En, MessageKey::ReasoningTruncated) => "(truncated to fit terminal height)",
        (Language::Zh, MessageKey::ReasoningTruncated) => "（内容过长，已按终端高度截断）",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_from_str() {
        assert!(matches!(Language::from_str("zh-CN"), Language::Zh));
        assert!(matches!(Language::from_str("zh_CN"), Language::Zh));
        assert!(matches!(Language::from_str("zh"), Language::Zh));
        assert!(matches!(Language::from_str("ZH-CN"), Language::Zh));
        assert!(matches!(Language::from_str("en-US"), Language::En));
        assert!(matches!(Language::from_str("en"), Language::En));
        assert!(matches!(Language::from_str("EN"), Language::En));
        assert!(matches!(Language::from_str("unknown"), Language::En));
    }

    #[test]
    fn test_translation() {
        assert_eq!(t(&Language::En, MessageKey::PromptUser), "you> ");
        assert_eq!(t(&Language::Zh, MessageKey::PromptUser), "你> ");
        assert_eq!(t(&Language::Zh, MessageKey::ThinkingProcess), "[思考中] ");
    }
}
