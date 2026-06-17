use std::collections::VecDeque;
use std::io::{self, Write};

use super::ansi::{strip, terminal_width};

const MAX_LINES: usize = 10;
const PREFIX_PLAIN: &str = "   │ ";
const PREFIX_COLOR: &str = "   \x1b[1;31m│\x1b[0m "; // red pipe on TTY

pub struct RollingDisplay {
    buf: VecDeque<String>,
    is_tty: bool,
    prefix: &'static str,
    col_width: usize,
    drawn: usize,
}

impl RollingDisplay {
    pub fn new(is_tty: bool) -> Self {
        let prefix = if is_tty { PREFIX_COLOR } else { PREFIX_PLAIN };
        let pfx_width = 5; // "   │ " = 5 display columns
        let col_width = terminal_width().saturating_sub(pfx_width);
        RollingDisplay {
            buf: VecDeque::with_capacity(MAX_LINES + 1),
            is_tty,
            prefix,
            col_width,
            drawn: 0,
        }
    }

    pub fn feed(&mut self, raw: &str, logfile: &mut impl Write) {
        let _ = writeln!(logfile, "{raw}");

        if !self.is_tty {
            println!("{raw}");
            return;
        }

        let line = raw.trim_end_matches('\r');
        let line = line.rsplit('\r').next().unwrap_or(line);
        let plain = strip(line);
        if plain.trim().is_empty() {
            return;
        }

        for chunk in wrap(&plain, self.col_width) {
            self.push_line(chunk);
        }
    }

    fn push_line(&mut self, line: String) {
        let stdout = io::stdout();
        let mut out = stdout.lock();

        if self.drawn > 0 {
            write!(out, "\x1b[{}A\x1b[J", self.drawn).unwrap();
        }

        self.buf.push_back(line);
        if self.buf.len() > MAX_LINES {
            self.buf.pop_front();
        }

        let pfx = self.prefix;
        for l in &self.buf {
            writeln!(out, "{pfx}{l}").unwrap();
        }
        self.drawn = self.buf.len();
        out.flush().unwrap();
    }
}

fn wrap(s: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![s.to_owned()];
    }
    let mut result = Vec::new();
    let mut chars = s.chars();
    loop {
        let chunk: String = chars.by_ref().take(width).collect();
        if chunk.is_empty() {
            break;
        }
        result.push(chunk);
    }
    result
}
