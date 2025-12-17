mod chat;
mod config;
mod i18n;
mod llm;
mod pty;

use std::env;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

use crate::chat::chat_mode;
use crate::config::{Config, SystemInfo, render_prompt};
use crate::i18n::Language;
use crate::llm::LLMClient;
use crate::llm::openai::OpenAIClient;
use crate::pty::PtySession;

fn main() -> Result<()> {
    let config = Config::load()?;
    let sys_info = SystemInfo::collect(config.preference.language.as_deref());

    let ui_lang = config
        .preference
        .language
        .as_deref()
        .map(Language::from_str)
        .unwrap_or_default();

    let system_prompt = render_prompt(&config.prompt.template, &sys_info.to_vars());

    let api_key = config
        .llm
        .api_key
        .or_else(|| env::var("OPENAI_API_KEY").ok())
        .context("OPENAI_API_KEY is required (set via config file or environment variable)")?;
    let model = config
        .llm
        .model
        .unwrap_or_else(|| env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string()));
    let base_url = config.llm.base_url.unwrap_or_else(|| {
        env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".to_string())
    });

    let llm: Box<dyn LLMClient> = Box::new(OpenAIClient::new(
        api_key,
        model,
        base_url,
        system_prompt,
    )?);

    let mut session = PtySession::new()?;
    session.spawn_output_relay()?;

    enable_raw_mode().context("failed to enter raw mode")?;
    let res = run_event_loop(&mut session, llm, ui_lang);
    disable_raw_mode().ok();
    res
}

fn run_event_loop(
    session: &mut PtySession,
    llm: Box<dyn LLMClient>,
    lang: Language,
) -> Result<()> {
    loop {
        if session.child_exited() {
            break;
        }

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Release {
                        continue;
                    }

                    // Ctrl+L enters LLM chat mode
                    if key.code == KeyCode::Char('l')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        let cmd = chat_mode(llm.as_ref(), &lang)?;
                        session.write(b"\r")?;
                        if let Some(cmd) = cmd {
                            session.write(cmd.as_bytes())?;
                        }
                        continue;
                    }

                    handle_key_event(session, key)?;
                }
                Event::Paste(text) => {
                    session.write(text.as_bytes())?;
                }
                Event::Resize(cols, rows) => {
                    session.resize(cols, rows);
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn handle_key_event(
    session: &mut PtySession,
    key: crossterm::event::KeyEvent,
) -> Result<()> {
    match key.code {
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                let ctrl_char = (c.to_ascii_lowercase() as u8) & 0x1f;
                session.write(&[ctrl_char])?;
            } else if key.modifiers.contains(KeyModifiers::ALT) {
                session.write(&[0x1b])?;
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                session.write(s.as_bytes())?;
            } else {
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                session.write(s.as_bytes())?;
            }
        }
        KeyCode::Enter => session.write(b"\r")?,
        KeyCode::Backspace => session.write(&[0x7f])?,
        KeyCode::Tab => session.write(b"\t")?,
        KeyCode::Esc => session.write(&[0x1b])?,
        KeyCode::Up => session.write(b"\x1b[A")?,
        KeyCode::Down => session.write(b"\x1b[B")?,
        KeyCode::Right => session.write(b"\x1b[C")?,
        KeyCode::Left => session.write(b"\x1b[D")?,
        KeyCode::Home => session.write(b"\x1b[H")?,
        KeyCode::End => session.write(b"\x1b[F")?,
        KeyCode::PageUp => session.write(b"\x1b[5~")?,
        KeyCode::PageDown => session.write(b"\x1b[6~")?,
        KeyCode::Delete => session.write(b"\x1b[3~")?,
        KeyCode::Insert => session.write(b"\x1b[2~")?,
        KeyCode::F(n) => {
            let seq = match n {
                1 => b"\x1bOP".as_slice(),
                2 => b"\x1bOQ".as_slice(),
                3 => b"\x1bOR".as_slice(),
                4 => b"\x1bOS".as_slice(),
                5 => b"\x1b[15~".as_slice(),
                6 => b"\x1b[17~".as_slice(),
                7 => b"\x1b[18~".as_slice(),
                8 => b"\x1b[19~".as_slice(),
                9 => b"\x1b[20~".as_slice(),
                10 => b"\x1b[21~".as_slice(),
                11 => b"\x1b[23~".as_slice(),
                12 => b"\x1b[24~".as_slice(),
                _ => return Ok(()),
            };
            session.write(seq)?;
        }
        _ => {}
    }
    Ok(())
}