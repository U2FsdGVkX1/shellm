mod chat;
mod config;
mod i18n;
mod llm;

use std::env;
use std::io::{self, Read, Write};
use std::thread;

use anyhow::{Context, Result};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use crate::chat::chat_mode;
use crate::config::{Config, SystemInfo, render_prompt};
use crate::i18n::Language;
use crate::llm::LLMClient;
use crate::llm::openai::OpenAIClient;

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

    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 32,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("failed to open pty")?;

    let cmd = CommandBuilder::new(shell);
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .context("failed to spawn shell")?;

    let mut writer = pair
        .master
        .take_writer()
        .context("failed to take pty writer")?;
    let mut reader = pair
        .master
        .try_clone_reader()
        .context("failed to clone pty reader")?;

    // Relay child output to stdout on a dedicated thread.
    thread::spawn(move || {
        let mut stdout = io::stdout();
        let mut buf = [0u8; 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let _ = stdout.write_all(&buf[..n]);
                    let _ = stdout.flush();
                }
                Err(_) => break,
            }
        }
    });

    enable_raw_mode().context("failed to enter raw mode")?;
    let res = run_event_loop(&mut writer, child.as_mut(), llm, ui_lang);
    disable_raw_mode().ok();
    res
}

fn run_event_loop<W: Write>(
    writer: &mut W,
    child: &mut dyn portable_pty::Child,
    llm: Box<dyn LLMClient>,
    lang: Language,
) -> Result<()> {
    let mut stdin = io::stdin();
    let mut buf = [0u8; 1];

    loop {
        if child
            .try_wait()
            .map(|status| status.is_some())
            .unwrap_or(false)
        {
            break;
        }

        let n = stdin.read(&mut buf).context("failed to read stdin")?;
        if n == 0 {
            continue;
        }

        let byte = buf[0];

        // Ctrl+L enters LLM chat mode
        if byte == 0x0c {
            let cmd = chat_mode(llm.as_ref(), &lang)?;
            writer.write_all(b"\x0d")?;
            if let Some(cmd) = cmd {
                writer.write_all(cmd.as_bytes())?;
            }
            writer.flush()?;
            continue;
        }

        writer.write_all(&buf[..n])?;
        writer.flush()?;
    }

    Ok(())
}
