use std::io::{self, Write};

use anyhow::Result;
use crossterm::{cursor, execute};
use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::terminal::{self, Clear, ClearType};

use crate::llm::{ChatMessage, ChatReply, LLMClient, Role};

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

fn prompt(buf: &str) {
    print!("\r\x1b[2Kyou> {buf}");
    io::stdout().flush().ok();
}

pub fn chat_mode(llm: &dyn LLMClient) -> Result<Option<String>> {
    print!(
        "\r\n\x1b[2K[LLM chat] Type your question. Ctrl+L accepts the command. Ctrl+C exits.\r\n"
    );

    let mut history: Vec<ChatMessage> = Vec::new();
    let mut last_cmd: Option<String> = None;
    let mut last_reasoning: Option<String> = None;
    let mut reasoning_expanded = false;
    let mut reasoning_anchor_saved = false;
    let mut buf = String::new();

    prompt(&buf);

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
        if let Event::Key(key) = evt {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Enter => {
                    if reasoning_expanded && reasoning_anchor_saved {
                        clear_reasoning_display(&mut reasoning_expanded, &mut reasoning_anchor_saved)?;
                        prompt(&buf);
                    }

                    print!("\r\n");
                    io::stdout().flush().ok();

                    let line = buf.trim_end().to_string();
                    if line.is_empty() {
                        buf.clear();
                        prompt(&buf);
                        continue;
                    }

                    // Get terminal width for sliding window
                    let term_width = get_terminal_width();
                    let thinking_text = "Thinking: ";
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
                    print!("assistant> {}\r\n", response.text.trim());
                    if !cmd.is_empty() {
                        print!("\x1b[2Kcandidate: {cmd}\r\n");
                        last_cmd = Some(cmd);
                    }
                    
                    // Show hint when reasoning can be expanded
                    if last_reasoning.is_some() {
                        print!("\x1b[90m(Press Ctrl+R to toggle reasoning display)\x1b[0m\r\n");
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
                    prompt(&buf);
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

                            print!("\x1b[90m--- Reasoning ---\r\n");
                            for line in reasoning.lines() {
                                print!("{}\r\n", line);
                            }
                            print!("--- End Reasoning ---\x1b[0m\r\n");
                            io::stdout().flush().ok();
                        } else {
                            // Collapse: restore prompt position and clear lines below
                            clear_reasoning_display(&mut reasoning_expanded, &mut reasoning_anchor_saved)?;
                        }
                        prompt(&buf);
                    }
                }
                KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if reasoning_expanded && reasoning_anchor_saved {
                        clear_reasoning_display(&mut reasoning_expanded, &mut reasoning_anchor_saved)?;
                        prompt(&buf);
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
                        prompt(&buf);
                    }
                }
                KeyCode::Char(c) => {
                    buf.push(c);
                    prompt(&buf);
                }
                _ => {}
            }
        }
    }
}
