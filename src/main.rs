mod chat;
mod llm;

use std::env;
use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{Context, Result};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use crate::chat::chat_mode;
use crate::llm::LLMClient;
use crate::llm::openai::OpenAIClient;

fn main() -> Result<()> {
    let api_key =
        env::var("OPENAI_API_KEY").context("OPENAI_API_KEY is required for OpenAI provider")?;
    let model = env::var("OPENAI_MODEL").unwrap_or("gpt-4o-mini".to_string());
    let base_url = env::var("OPENAI_BASE_URL")
        .unwrap_or("https://api.openai.com/v1".to_string());

    let system_prompt =
        "You are a focused shell copilot. Always respond ONLY with a JSON object: \
        {\"command\": \"<shell command>\", \"answer\": \"brief human-readable note\"}. \
        Do not add code fences or extra text. Prefer safe defaults; if unsure ask via answer."
            .to_string();

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
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("failed to open pty")?;

    let cmd = CommandBuilder::new(&shell);
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .context("failed to spawn shell")?;

    let mut reader = pair.master.try_clone_reader().context("failed to clone reader")?;
    let mut writer = pair.master.take_writer().context("failed to take writer")?;

    let output = Arc::new(Mutex::new(io::stdout()));
    let output_clone = Arc::clone(&output);
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let mut out = output_clone.lock().unwrap();
                    out.write_all(&buf[..n]).ok();
                    out.flush().ok();
                }
            }
        }
    });

    enable_raw_mode().context("failed to enter raw mode")?;
    let res = run_event_loop(&mut writer, child.as_mut(), llm);
    disable_raw_mode().ok();
    res
}

fn run_event_loop<W: Write>(
    writer: &mut W,
    child: &mut dyn portable_pty::Child,
    llm: Box<dyn LLMClient>,
) -> Result<()> {
    let mut stdin = io::stdin();
    let mut buf = [0u8; 1];

    loop {
        if let Ok(Some(_status)) = child.try_wait() {
            break;
        }

        if stdin.read(&mut buf).is_err() {
            continue;
        }

        let byte = buf[0];

        if byte != 0x0c {
            writer.write_all(&buf)?;
            writer.flush()?;
            continue;
        }

        // Ctrl+L enters LLM chat mode
        if byte == 0x0c {
            let cmd = chat_mode(llm.as_ref())?;
            writer.write_all(b"\x0d")?;
            if let Some(cmd) = cmd {
                writer.write_all(cmd.as_bytes())?;
            }
            writer.flush()?;
        }
    }
    Ok(())
}
