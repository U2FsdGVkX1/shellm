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

fn approx_char_width(c: char) -> usize {
    match c {
        '\u{0000}'..='\u{001F}' | '\u{007F}' => 0,
        _ if c.is_ascii() => 1,
        _ => 2,
    }
}

fn approx_display_width(s: &str) -> usize {
    s.chars().map(approx_char_width).sum()
}

fn wrap_rows(visible: &str, cols: usize) -> usize {
    if cols == 0 {
        return 1;
    }
    let width = approx_display_width(visible);
    (width.max(1) + cols - 1) / cols
}

fn truncate_tail_by_width(s: &str, max_width: usize) -> &str {
    if max_width == 0 {
        return "";
    }
    if approx_display_width(s) <= max_width {
        return s;
    }

    let mut width = 0usize;
    let mut start = s.len();
    for (idx, ch) in s.char_indices().rev() {
        let w = approx_char_width(ch);
        if width + w > max_width {
            break;
        }
        width += w;
        start = idx;
    }
    &s[start..]
}

fn prompt(buf: &str, lang: &Language) {
    let prompt_text = t(lang, MessageKey::PromptUser);
    let term_cols = get_terminal_width();
    let prompt_width = approx_display_width(prompt_text);
    let max_buf_width = term_cols.saturating_sub(prompt_width).saturating_sub(1);
    let display = truncate_tail_by_width(buf, max_buf_width);
    print!("\r\x1b[2K{prompt_text}{display}");
    io::stdout().flush().ok();
}

fn normalize_to_single_line(s: &str) -> String {
    s.chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Pre-compute the number of rows needed to render the reply block (without truncation)
fn calculate_reply_rows(
    lang: &Language,
    reasoning: Option<&str>,
    reasoning_expanded: bool,
    answer: &str,
    cmd: Option<&str>,
    term_cols: usize,
) -> usize {
    let answer = normalize_to_single_line(answer);
    let cmd = cmd.map(normalize_to_single_line);

    let assistant_prompt = t(lang, MessageKey::PromptAssistant);
    let assistant_visible = format!("{assistant_prompt}{answer}");
    let assistant_rows = wrap_rows(&assistant_visible, term_cols);

    let candidate_rows = if let Some(cmd) = cmd.as_deref().filter(|s| !s.is_empty()) {
        let candidate_prompt = t(lang, MessageKey::PromptCandidate);
        let visible = format!("{candidate_prompt}{cmd}");
        wrap_rows(&visible, term_cols)
    } else {
        0
    };

    let reasoning_rows = if let Some(reasoning) = reasoning {
        if reasoning_expanded {
            let reasoning_start = t(lang, MessageKey::ReasoningStart);
            let reasoning_end = t(lang, MessageKey::ReasoningEnd);
            let start_rows = wrap_rows(reasoning_start, term_cols);
            let end_rows = wrap_rows(reasoning_end, term_cols);

            // Number of rows for reasoning content
            let content_rows: usize = reasoning.lines().map(|l| wrap_rows(l, term_cols)).sum();

            // Possible truncation hint
            let truncated_hint = t(lang, MessageKey::ReasoningTruncated);
            let truncated_rows = wrap_rows(truncated_hint, term_cols);

            start_rows + content_rows + truncated_rows + end_rows
        } else {
            let hint = t(lang, MessageKey::HintToggleReasoning);
            wrap_rows(hint, term_cols)
        }
    } else {
        0
    };

    reasoning_rows + assistant_rows + candidate_rows
}

/// Ensure there is enough space to render content, scrolling the terminal when needed.
/// Returns the actual number of lines scrolled.
fn ensure_scroll_space(stdout: &mut io::Stdout, needed_rows: usize) -> Result<usize> {
    let (_, term_rows) = terminal::size().unwrap_or((80, 24));
    let (_, cur_row) = cursor::position().unwrap_or((0, 0));

    // Available rows below the cursor (minus one line reserved for the input prompt)
    let available_below = (term_rows.saturating_sub(cur_row + 1)) as usize;

    if needed_rows > available_below {
        // Need to scroll: compute how many lines must be freed
        let scroll_lines = needed_rows - available_below;

        // Print blank lines so the terminal scrolls upward automatically
        for _ in 0..scroll_lines {
            print!("\r\n");
        }
        stdout.flush()?;

        // Move the cursor back to the rendering start position
        execute!(stdout, cursor::MoveUp(scroll_lines as u16))?;

        Ok(scroll_lines)
    } else {
        Ok(0)
    }
}

fn render_reply_block(
    lang: &Language,
    reasoning: Option<&str>,
    reasoning_expanded: bool,
    answer: &str,
    cmd: Option<&str>,
    term_cols: usize,
    max_rows: usize,
) -> usize {
    let answer = normalize_to_single_line(answer);
    let cmd = cmd.map(normalize_to_single_line);

    let assistant_prompt = t(lang, MessageKey::PromptAssistant);
    let assistant_visible = format!("{assistant_prompt}{answer}");
    let assistant_rows = wrap_rows(&assistant_visible, term_cols);

    let (candidate_visible, candidate_rows) =
        if let Some(cmd) = cmd.as_deref().filter(|s| !s.is_empty()) {
            let candidate_prompt = t(lang, MessageKey::PromptCandidate);
            let visible = format!("{candidate_prompt}{cmd}");
            let rows = wrap_rows(&visible, term_cols);
            (Some(visible), rows)
        } else {
            (None, 0)
        };

    let mut used_rows = 0usize;

    if let Some(reasoning) = reasoning {
        if reasoning_expanded {
            let reasoning_start = t(lang, MessageKey::ReasoningStart);
            let reasoning_end = t(lang, MessageKey::ReasoningEnd);
            let start_rows = wrap_rows(reasoning_start, term_cols);
            let end_rows = wrap_rows(reasoning_end, term_cols);

            // Reserve space for assistant/candidate and start/end markers.
            let reserved = assistant_rows + candidate_rows + start_rows + end_rows;
            if reserved >= max_rows {
                let hint = t(lang, MessageKey::HintToggleReasoning);
                print!("\x1b[90m{}\x1b[0m\r\n", hint);
                used_rows += wrap_rows(hint, term_cols);
            } else {
                let mut budget = max_rows - reserved;

                let reasoning_lines: Vec<&str> = reasoning.lines().collect();
                let total_reasoning_rows: usize =
                    reasoning_lines.iter().map(|l| wrap_rows(l, term_cols)).sum();

                let show_truncated = total_reasoning_rows > budget;
                let truncated_hint = t(lang, MessageKey::ReasoningTruncated);
                let truncated_rows = wrap_rows(truncated_hint, term_cols);

                if show_truncated {
                    if truncated_rows >= budget {
                        budget = 0;
                    } else {
                        budget -= truncated_rows;
                    }
                }

                print!("\x1b[90m{}\r\n", reasoning_start);
                used_rows += start_rows;
                if show_truncated {
                    print!("\x1b[90m{}\x1b[0m\r\n", truncated_hint);
                    used_rows += truncated_rows;
                }

                if budget > 0 {
                    let mut content_used_rows = 0usize;
                    let mut selected: Vec<String> = Vec::new();
                    for line in reasoning_lines.iter().rev() {
                        let rows = wrap_rows(line, term_cols);
                        if content_used_rows + rows <= budget {
                            selected.push((*line).to_string());
                            content_used_rows += rows;
                            continue;
                        }

                        let remaining_rows = budget.saturating_sub(content_used_rows);
                        if remaining_rows == 0 {
                            break;
                        }

                        let max_width = remaining_rows.saturating_mul(term_cols);
                        let truncated = truncate_tail_by_width(line, max_width);
                        if !truncated.is_empty() {
                            selected.push(truncated.to_string());
                            content_used_rows += remaining_rows;
                        }
                        break;
                    }
                    selected.reverse();
                    for line in selected {
                        print!("{line}\r\n");
                    }
                    used_rows += content_used_rows;
                }

                print!("{}\x1b[0m\r\n", reasoning_end);
                used_rows += end_rows;
            }
        } else {
            let hint = t(lang, MessageKey::HintToggleReasoning);
            print!("\x1b[90m{}\x1b[0m\r\n", hint);
            used_rows += wrap_rows(hint, term_cols);
        }
    }

    print!("{}{}\r\n", assistant_prompt, answer);
    used_rows += assistant_rows;

    if let Some(visible) = candidate_visible {
        print!("\x1b[2K{visible}\r\n");
        used_rows += candidate_rows;
    }

    used_rows
}

pub fn chat_mode(llm: &dyn LLMClient, lang: &Language) -> Result<Option<String>> {
    let welcome = t(lang, MessageKey::WelcomeMessage);
    print!("\r\n\x1b[2K{welcome}\r\n");

    let _paste_guard = BracketedPasteGuard::enable()?;
    let mut history: Vec<ChatMessage> = Vec::new();
    let mut last_cmd: Option<String> = None;
    let mut last_answer: Option<String> = None;
    let mut last_reasoning: Option<String> = None;
    let mut reasoning_expanded = false;
    let mut last_reply_rows = 0usize;
    let mut buf = String::new();

    prompt(&buf, lang);

    loop {
        let evt = event::read()?;
        match evt {
            Event::Key(key) => {
                if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                    continue;
                }
                match key.code {
                KeyCode::Enter => {
                    print!("\r\n");
                    io::stdout().flush().ok();

                    let line = buf.trim_end().to_string();
                    if line.is_empty() {
                        buf.clear();
                        prompt(&buf, lang);
                        continue;
                    }

                    // Get terminal width for sliding window (keep in a single terminal row)
                    let thinking_text = t(lang, MessageKey::ThinkingProcess);
                    let prefix = format!("\x1b[90m{}", thinking_text);
                    let prefix_width = approx_display_width(thinking_text);

                    let mut clean_reasoning_buffer = String::new();
                    let mut has_reasoning = false;
                    
                    // Create callback to display reasoning in real time (single-line sliding window)
                    let mut reasoning_callback = |reasoning: &str| {
                        has_reasoning = true;
                        
                        // Strip newlines incrementally and show as a single line
                        for c in reasoning.chars() {
                            clean_reasoning_buffer.push(if c == '\n' || c == '\r' { ' ' } else { c });
                        }
                        
                        // Display only the tail that fits in the current terminal width
                        let term_width = get_terminal_width();
                        let max_display_width = term_width
                            .saturating_sub(prefix_width)
                            .saturating_sub(1);
                        let display = truncate_tail_by_width(&clean_reasoning_buffer, max_display_width);
                        
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

                    last_answer = Some(response.text.clone());
                    last_cmd = response
                        .suggested_command
                        .clone()
                        .filter(|cmd| !cmd.is_empty());

                    let mut stdout = io::stdout();
                    execute!(stdout, cursor::MoveToColumn(0), Clear(ClearType::FromCursorDown))?;

                    let (cols, rows) = terminal::size().unwrap_or((80, 24));

                    // Pre-compute how many rows are needed
                    let needed_rows = calculate_reply_rows(
                        lang,
                        last_reasoning.as_deref(),
                        reasoning_expanded,
                        last_answer.as_deref().unwrap_or(""),
                        last_cmd.as_deref(),
                        cols as usize,
                    );

                    // Ensure there is enough space
                    ensure_scroll_space(&mut stdout, needed_rows)?;

                    // Use full terminal height as max_rows (space has been ensured)
                    let max_rows = rows as usize;

                    last_reply_rows = render_reply_block(
                        lang,
                        last_reasoning.as_deref(),
                        reasoning_expanded,
                        last_answer.as_deref().unwrap_or(""),
                        last_cmd.as_deref(),
                        cols as usize,
                        max_rows,
                    );
                    io::stdout().flush().ok();
                    
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
                    if last_reasoning.is_some() && last_reply_rows > 0 {
                        reasoning_expanded = !reasoning_expanded;

                        let (cols, rows) = terminal::size().unwrap_or((80, 24));
                        let mut stdout = io::stdout();

                        // Step 1: clear the previous reply block
                        execute!(stdout, cursor::MoveToColumn(0))?;
                        execute!(
                            stdout,
                            cursor::MoveUp(last_reply_rows.min(u16::MAX as usize) as u16),
                            Clear(ClearType::FromCursorDown)
                        )?;

                        // Step 2: pre-compute how many rows are needed
                        let needed_rows = calculate_reply_rows(
                            lang,
                            last_reasoning.as_deref(),
                            reasoning_expanded,
                            last_answer.as_deref().unwrap_or(""),
                            last_cmd.as_deref(),
                            cols as usize,
                        );

                        // Step 3: ensure there is enough space
                        ensure_scroll_space(&mut stdout, needed_rows)?;

                        // Step 4: render the reply block (using full terminal height as max_rows)
                        let max_rows = rows as usize;

                        last_reply_rows = render_reply_block(
                            lang,
                            last_reasoning.as_deref(),
                            reasoning_expanded,
                            last_answer.as_deref().unwrap_or(""),
                            last_cmd.as_deref(),
                            cols as usize,
                            max_rows,
                        );
                        io::stdout().flush().ok();

                        prompt(&buf, lang);
                    }
                }
                KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if let Some(ref cmd) = last_cmd {
                        return Ok(Some(cmd.clone()));
                    }
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
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
