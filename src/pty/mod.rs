mod responder;

use responder::VtResponder;
use std::env;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, MasterPty, PtyPair, PtySize, native_pty_system};

pub type PtyWriter = Arc<Mutex<Box<dyn Write + Send>>>;

pub struct PtySession {
    pub master: Box<dyn MasterPty + Send>,
    pub child: Box<dyn portable_pty::Child + Send + Sync>,
    pub writer: PtyWriter,
}

impl PtySession {
    pub fn new() -> Result<Self> {
        let shell = detect_shell();
        let (cols, rows) = crossterm::terminal::size().unwrap_or((120, 32));

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to open pty")?;

        let PtyPair { master, slave } = pair;
        let current_dir = env::current_dir().context("failed to get current directory")?;

        let mut cmd = CommandBuilder::new(&shell);
        cmd.cwd(current_dir);

        let child = slave
            .spawn_command(cmd)
            .context("failed to spawn shell")?;

        let writer = master.take_writer().context("failed to take pty writer")?;
        let writer: PtyWriter = Arc::new(Mutex::new(writer));

        Ok(Self {
            master,
            child,
            writer,
        })
    }

    pub fn spawn_output_relay(&self) -> Result<()> {
        let mut reader = self
            .master
            .try_clone_reader()
            .context("failed to clone pty reader")?;
        let writer_for_responder = self.writer.clone();

        thread::spawn(move || {
            let mut stdout = std::io::stdout();
            let mut buf = [0u8; 1024];
            let mut responder = VtResponder::new();

            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let filtered = responder.process(&buf[..n], |resp| {
                            let _ = write_bytes(&writer_for_responder, resp);
                        });
                        let _ = stdout.write_all(&filtered);
                        let _ = stdout.flush();
                    }
                    Err(_) => break,
                }
            }

            let _ = responder.finish(|tail| {
                let _ = stdout.write_all(tail);
                let _ = stdout.flush();
            });
        });

        Ok(())
    }

    pub fn child_exited(&mut self) -> bool {
        self.child
            .try_wait()
            .map(|status| status.is_some())
            .unwrap_or(false)
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
    }

    pub fn write(&self, bytes: &[u8]) -> Result<()> {
        write_bytes(&self.writer, bytes)
    }
}

fn detect_shell() -> String {
    #[cfg(target_os = "windows")]
    {
        detect_windows_shell()
    }

    #[cfg(not(target_os = "windows"))]
    {
        detect_unix_shell()
    }
}

#[cfg(target_os = "windows")]
fn detect_windows_shell() -> String {
    if env::var("PSModulePath").is_ok() {
        "powershell.exe".to_string()
    } else {
        "cmd.exe".to_string()
    }
}

#[cfg(not(target_os = "windows"))]
fn detect_unix_shell() -> String {
    env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
}

fn write_bytes(writer: &PtyWriter, bytes: &[u8]) -> Result<()> {
    let mut w = writer
        .lock()
        .map_err(|_| anyhow::anyhow!("pty writer poisoned"))?;
    w.write_all(bytes)?;
    w.flush()?;
    Ok(())
}