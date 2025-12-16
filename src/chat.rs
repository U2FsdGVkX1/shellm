use std::io::{self, Write};

use anyhow::Result;
use crossterm::{cursor, execute};
use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::terminal::{self, Clear, ClearType};

use crate::i18n::{Language, MessageKey, t};
use crate::llm::{ChatMessage, ChatReply, LLMClient, Role};

struct BracketedPasteGuard;

impl BracketedPasteGuard {
    fn enable() -> Result<Self> {
        let mut stdout = io::stdout();
        execute!(stdout, EnableBracketedPaste)?;
        Ok(Self)
    }
}

impl Drop for BracketedPasteGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = execute!(stdout, DisableBracketedPaste);
    }
}

/// Get terminal width, default 80
fn get_terminal_width() -> usize {
    terminal::size().map(|(w, _)| w as usize).unwrap_or(80)
}

/// Take last N characters of a string (by Unicode characters)
fn truncate_tail(s: &str, max_chars: usize) -> &str {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        return s;
    }
    let skip = char_count - max_chars;
    let mut chars = s.chars();
    for _ in 0..skip {
        chars.next();
    }
    chars.as_str()
}

fn prompt(buf: &str, lang: &Language) {
    let prompt_text = t(lang, MessageKey::PromptUser);
    print!("\r\x1b[2K{prompt_text}{buf}");
    io::stdout().flush().ok();
}

pub fn chat_mode(llm: &dyn LLMClient, lang: &Language) -> Result<Option<String>> {
    let welcome = t(lang, MessageKey::WelcomeMessage);
    print!("\r\n\x1b[2K{welcome}\r\n");

    let _paste_guard = BracketedPasteGuard::enable()?;
    let mut history: Vec<ChatMessage> = Vec::new();
    let mut last_cmd: Option<String> = None;
    let mut last_reasoning: Option<String> = None;
    let mut reasoning_expanded = false;
    let mut reasoning_anchor_saved = false;
    let mut buf = String::new();

    prompt(&buf, lang);

    let clear_reasoning_display = |expanded: &mut bool, saved: &mut bool| -> Result<()> {
        if *expanded && *saved {
            let mut stdout = io::stdout();
            execute!(
                stdout,
                cursor::RestorePosition,
                Clear(ClearType::FromCursorDown)
            )?;
            *expanded = false;
            *saved = false;
        }
        Ok(())
    };

    loop {
        let evt = event::read()?;
        match evt {
            Event::Key(key) => {
                if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                    continue;
                }
                match key.code {
                KeyCode::Enter => {
                    if reasoning_expanded && reasoning_anchor_saved {
                        clear_reasoning_display(&mut reasoning_expanded, &mut reasoning_anchor_saved)?;
                        prompt(&buf, lang);
                    }

                    print!("\r\n");
                    io::stdout().flush().ok();

                    let line = buf.trim_end().to_string();
                    if line.is_empty() {
                        buf.clear();
                        prompt(&buf, lang);
                        continue;
                    }

                    // Get terminal width for sliding window
                    let term_width = get_terminal_width();
                    let thinking_text = t(lang, MessageKey::ThinkingProcess);
                    let prefix = format!("\x1b[90m{}", thinking_text);
                    let prefix_visible_len = thinking_text.chars().count();
                    let max_display_chars = (term_width.saturating_sub(prefix_visible_len * 2)).max(20);
                    
                    let mut reasoning_buffer = String::new();
                    let mut has_reasoning = false;
                    
                    // Create callback to display reasoning in real time (single-line sliding window)
                    let mut reasoning_callback = |reasoning: &str| {
                        reasoning_buffer.push_str(reasoning);
                        has_reasoning = true;
                        
                        // Strip newlines and show as a single line
                        let clean_reasoning: String = reasoning_buffer
                            .chars()
                            .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
                            .collect();
                        
                        // Display only the tail of the text
                        let display = truncate_tail(&clean_reasoning, max_display_chars);
                        
                        // Use \r to overwrite the current line
                        print!("\r\x1b[2K{}{}\x1b[0m", &prefix, display);
                        io::stdout().flush().ok();
                    };

                    let response: ChatReply = llm.chat(&history, &line, &mut reasoning_callback)?;
                    
                    // Clear the reasoning display line
                    if has_reasoning {
                        print!("\r\x1b[2K");
                        io::stdout().flush().ok();
                    }
                    
                    // Save full reasoning so Ctrl+R can expand it
                    last_reasoning = response.reasoning.clone();
                    reasoning_expanded = false;
                    reasoning_anchor_saved = false;
                    
                    let cmd = response.suggested_command.clone().unwrap_or_default();
                    let assistant_prompt = t(lang, MessageKey::PromptAssistant);
                    print!("{}{}\r\n", assistant_prompt, response.text.trim());
                    if !cmd.is_empty() {
                        let candidate_prompt = t(lang, MessageKey::PromptCandidate);
                        print!("\x1b[2K{}{cmd}\r\n", candidate_prompt);
                        last_cmd = Some(cmd);
                    }
                    
                    // Show hint when reasoning can be expanded
                    if last_reasoning.is_some() {
                        let hint = t(lang, MessageKey::HintToggleReasoning);
                        print!("\x1b[90m{}\x1b[0m\r\n", hint);
                    }
                    
                    history.push(ChatMessage {
                        role: Role::User,
                        content: line,
                    });
                    history.push(ChatMessage {
                        role: Role::Assistant,
                        content: response.text,
                    });

                    buf.clear();
                    prompt(&buf, lang);
                }
                KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    // Toggle reasoning expansion/collapse
                    if let Some(ref reasoning) = last_reasoning {
                        reasoning_expanded = !reasoning_expanded;
                        if reasoning_expanded {
                            // Expand: clear lines below prompt and render reasoning here
                            let mut stdout = io::stdout();
                            execute!(
                                stdout,
                                cursor::MoveToColumn(0),
                                cursor::SavePosition,
                                Clear(ClearType::FromCursorDown)
                            )?;
                            reasoning_anchor_saved = true;

                            let reasoning_start = t(lang, MessageKey::ReasoningStart);
                            let reasoning_end = t(lang, MessageKey::ReasoningEnd);
                            print!("\x1b[90m{}\r\n", reasoning_start);
                            for line in reasoning.lines() {
                                print!("{}\r\n", line);
                            }
                            print!("{}\x1b[0m\r\n", reasoning_end);
                            io::stdout().flush().ok();
                        } else {
                            // Collapse: restore prompt position and clear lines below
                            clear_reasoning_display(&mut reasoning_expanded, &mut reasoning_anchor_saved)?;
                        }
                        prompt(&buf, lang);
                    }
                }
                KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if reasoning_expanded && reasoning_anchor_saved {
                        clear_reasoning_display(&mut reasoning_expanded, &mut reasoning_anchor_saved)?;
                        prompt(&buf, lang);
                    }
                    if let Some(ref cmd) = last_cmd {
                        return Ok(Some(cmd.clone()));
                    }
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if reasoning_expanded && reasoning_anchor_saved {
                         clear_reasoning_display(&mut reasoning_expanded, &mut reasoning_anchor_saved)?;
                    }
                    return Ok(None);
                }
                KeyCode::Backspace => {
                    if !buf.is_empty() {
                        buf.pop();
                        prompt(&buf, lang);
                    }
                }
                KeyCode::Char(c) => {
                    buf.push(c);
                    prompt(&buf, lang);
                }
                _ => {}
                }
            }
            Event::Paste(pasted) => {
                let normalized = pasted.replace(['\r', '\n'], " ");
                buf.push_str(&normalized);
                prompt(&buf, lang);
            }
            _ => {}
        }
    }
}
