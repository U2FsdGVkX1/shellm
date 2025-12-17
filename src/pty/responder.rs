use anyhow::Result;

pub struct VtResponder {
    pending: Vec<u8>,
}

impl VtResponder {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    pub fn process(&mut self, chunk: &[u8], mut on_response: impl FnMut(&[u8])) -> Vec<u8> {
        self.pending.extend_from_slice(chunk);
        let mut out: Vec<u8> = Vec::with_capacity(chunk.len());

        let mut i = 0usize;
        while i < self.pending.len() {
            if self.pending[i] != 0x1b {
                out.push(self.pending[i]);
                i += 1;
                continue;
            }

            // Escape sequence start; buffer until we have a full sequence
            if i + 1 >= self.pending.len() {
                break;
            }

            match self.pending[i + 1] {
                b'[' => {
                    let Some(end) = parse_csi_end(&self.pending, i + 2) else {
                        break;
                    };
                    let seq = &self.pending[i..=end];

                    // 拦截常见的终端查询序列并响应
                    if seq == b"\x1b[6n" {
                        // DSR (Device Status Report) - Cursor Position
                        let resp = cursor_position_response();
                        on_response(&resp);
                    } else if seq == b"\x1b[5n" {
                        // DSR - Device Status
                        on_response(b"\x1b[0n");
                    } else if seq == b"\x1b[c" {
                        // DA1 (Primary Device Attributes)
                        on_response(b"\x1b[?1;0c");
                    } else {
                        out.extend_from_slice(seq);
                    }
                    i = end + 1;
                }
                b']' => {
                    let Some(end) = parse_osc_end(&self.pending, i + 2) else {
                        break;
                    };
                    out.extend_from_slice(&self.pending[i..=end]);
                    i = end + 1;
                }
                b'P' | b'X' | b'^' | b'_' => {
                    let Some(end) = parse_st_terminated(&self.pending, i + 2) else {
                        break;
                    };
                    out.extend_from_slice(&self.pending[i..=end]);
                    i = end + 1;
                }
                _ => {
                    let Some(end) = parse_esc(&self.pending, i + 1) else {
                        break;
                    };
                    out.extend_from_slice(&self.pending[i..=end]);
                    i = end + 1;
                }
            }
        }

        self.pending = self.pending[i..].to_vec();
        out
    }

    pub fn finish(&mut self, mut on_tail: impl FnMut(&[u8])) -> Result<()> {
        if let Some(pos) = self.pending.iter().position(|b| *b == 0x1b) {
            if pos > 0 {
                on_tail(&self.pending[..pos]);
            }
        } else if !self.pending.is_empty() {
            on_tail(&self.pending);
        }
        self.pending.clear();
        Ok(())
    }
}

impl Default for VtResponder {
    fn default() -> Self {
        Self::new()
    }
}

fn cursor_position_response() -> Vec<u8> {
    if let Ok((col, row)) = crossterm::cursor::position() {
        format!(
            "\x1b[{};{}R",
            row.saturating_add(1),
            col.saturating_add(1)
        )
        .into_bytes()
    } else {
        b"\x1b[1;1R".to_vec()
    }
}

// CSI: ESC [ ... <final> (0x40..0x7E)
fn parse_csi_end(buf: &[u8], start: usize) -> Option<usize> {
    for idx in start..buf.len() {
        let b = buf[idx];
        if (0x40..=0x7e).contains(&b) {
            return Some(idx);
        }
    }
    None
}

// OSC: ESC ] ... BEL | ST(ESC \)
fn parse_osc_end(buf: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i < buf.len() {
        match buf[i] {
            0x07 => return Some(i), // BEL
            0x1b => {
                if i + 1 < buf.len() && buf[i + 1] == b'\\' {
                    return Some(i + 1); // ST
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

// DCS/SOS/PM/APC: ESC <X> ... ST(ESC \)
fn parse_st_terminated(buf: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i + 1 < buf.len() {
        if buf[i] == 0x1b && buf[i + 1] == b'\\' {
            return Some(i + 1);
        }
        i += 1;
    }
    None
}

// ESC + intermediates(0x20..0x2F)* + final(0x30..0x7E)
fn parse_esc(buf: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i < buf.len() {
        let b = buf[i];
        if (0x20..=0x2f).contains(&b) {
            i += 1;
            continue;
        }
        return Some(i);
    }
    None
}