use std::path::Path;

use super::ansi::terminal_width;

// ── TTY detection ─────────────────────────────────────────────────────────────

pub fn stderr_is_tty() -> bool {
    unsafe { libc::isatty(libc::STDERR_FILENO) != 0 }
}

pub fn stdout_is_tty() -> bool {
    unsafe { libc::isatty(libc::STDOUT_FILENO) != 0 }
}

pub fn stdin_is_tty() -> bool {
    unsafe { libc::isatty(libc::STDIN_FILENO) != 0 }
}

// ── Color palette ─────────────────────────────────────────────────────────────

pub struct Colors {
    pub b: &'static str,
    pub mg: &'static str,
    pub rd: &'static str,
    pub yl: &'static str,
    pub yln: &'static str,
    pub cy: &'static str,
    pub cyb: &'static str,
    pub gr: &'static str,
    pub dim: &'static str,
    pub r: &'static str,
}

impl Colors {
    pub fn stderr() -> Self {
        if stderr_is_tty() {
            Self::on()
        } else {
            Self::off()
        }
    }
    pub fn stdout() -> Self {
        if stdout_is_tty() {
            Self::on()
        } else {
            Self::off()
        }
    }

    fn on() -> Self {
        Self {
            b: "\x1b[1m",
            mg: "\x1b[1;35m",
            rd: "\x1b[1;31m",
            yl: "\x1b[1;33m",
            yln: "\x1b[0;33m",
            cy: "\x1b[0;36m",
            cyb: "\x1b[1;36m",
            gr: "\x1b[0;32m",
            dim: "\x1b[2m",
            r: "\x1b[0m",
        }
    }

    fn off() -> Self {
        Self {
            b: "",
            mg: "",
            rd: "",
            yl: "",
            yln: "",
            cy: "",
            cyb: "",
            gr: "",
            dim: "",
            r: "",
        }
    }
}

// ── Line extension (fills to terminal width) ──────────────────────────────────

pub fn line_ext() -> String {
    let cols = terminal_width();
    let mut out = String::new();
    let mut offset = 90usize;
    while offset < cols {
        out.push_str("────────────────────");
        offset += 10;
    }
    out
}

// ── Diagnostic messages ───────────────────────────────────────────────────────

pub fn die(msg: &str) -> ! {
    let c = Colors::stderr();
    eprintln!(
        "\n {rd}{b}✗  error:{r}  {msg}\n",
        rd = c.rd,
        b = c.b,
        r = c.r
    );
    std::process::exit(1);
}

pub fn warn(msg: &str) {
    let c = Colors::stderr();
    eprintln!(" {yl}{b}⚠  warning:{r}  {msg}", yl = c.yl, b = c.b, r = c.r);
}

// ── Container lifecycle box ───────────────────────────────────────────────────

pub struct LifecycleBox<'a> {
    pub action: &'a str,
    pub container: &'a str,
    pub image: Option<&'a str>,
    pub setup_cmd: Option<&'a str>,
    pub platform: Option<&'a str>,
}

pub fn print_lifecycle_box(b: &LifecycleBox) {
    let c = Colors::stderr();
    let ext = line_ext();
    let line = format!("─────────────────────────────────────────────────────────────{ext}");
    let footer =
        format!("└───────────────────────────────────────────────────────────────────{ext}");

    eprintln!("\n{mg} ┌─ shrike {line}{r}", mg = c.mg, r = c.r);
    eprintln!(
        "{mg} │{r} {b}{name:<8}:{r} {yl}{container}{r}",
        mg = c.mg,
        b = c.b,
        r = c.r,
        yl = c.yl,
        name = format!("{} ", b.action),
        container = b.container,
    );
    if let Some(img) = b.image {
        eprintln!(
            "{mg} │{r} {b}{name:<8}:{r} {yl}{img}{r}",
            mg = c.mg,
            b = c.b,
            r = c.r,
            yl = c.yl,
            name = "Image   ",
        );
    }
    if let Some(cmd) = b.setup_cmd {
        eprintln!(
            "{mg} │{r} {b}{name:<8}:{r} {yl}{cmd}{r}",
            mg = c.mg,
            b = c.b,
            r = c.r,
            yl = c.yl,
            name = "Setup   ",
        );
    }
    if let Some(cmd) = b.platform {
        eprintln!(
            "{mg} │{r} {b}{name:<8}:{r} {yl}{cmd}{r}",
            mg = c.mg,
            b = c.b,
            r = c.r,
            yl = c.yl,
            name = "Platform",
        );
    }
    eprintln!("{rd} {footer}{r}", rd = c.rd, r = c.r);
}

// ── Pre-execution summary ─────────────────────────────────────────────────────

pub struct SummaryInfo<'a> {
    pub profile: &'a str,
    pub image: Option<&'a str>,
    pub container: Option<&'a str>,
    pub ports: &'a [String],
    pub volumes: &'a [String],
    pub workdir: &'a str,
    pub cmd_display: &'a str,
    pub env_display: &'a str,
    pub user: Option<&'a str>,
    pub interactive: bool,
}

pub fn print_summary(s: &SummaryInfo) {
    let c = Colors::stderr();
    let ext = line_ext();
    let line = format!("─────────────────────────────────────────────────────────────{ext}");
    let footer =
        format!("└───────────────────────────────────────────────────────────────────{ext}");

    eprintln!("\n{mg} ┌─ shrike {line}{r}", mg = c.mg, r = c.r);
    eprintln!(
        "{mg} │{r} {b}Profile   :{r} {yl}{p}{r}",
        mg = c.mg,
        b = c.b,
        r = c.r,
        yl = c.yl,
        p = s.profile,
    );
    if let Some(img) = s.image {
        eprintln!(
            "{mg} │{r} {b}Image     :{r} {img}",
            mg = c.mg,
            b = c.b,
            r = c.r,
        );
    }
    if let Some(ct) = s.container {
        eprintln!(
            "{mg} │{r} {b}Container :{r} {dim}{ct}{r}",
            mg = c.mg,
            b = c.b,
            r = c.r,
            dim = c.dim,
        );
    }
    if !s.ports.is_empty() {
        eprintln!(
            "{mg} │{r} {b}Ports     :{r} {gr}{v}{r}",
            mg = c.mg,
            b = c.b,
            r = c.r,
            gr = c.gr,
            v = s.ports.join(", "),
        );
    }
    if !s.volumes.is_empty() {
        eprintln!(
            "{mg} │{r} {b}Volumes   :{r} {gr}{v}{r}",
            mg = c.mg,
            b = c.b,
            r = c.r,
            gr = c.gr,
            v = s.volumes.join(", "),
        );
    }
    eprintln!(
        "{mg} │{r} {b}Directory :{r} {gr}{wd}{r}",
        mg = c.mg,
        b = c.b,
        r = c.r,
        gr = c.gr,
        wd = s.workdir,
    );
    if s.interactive {
        eprintln!(
            "{mg} │{r} {b}Command   :{r} {yl}{cmd}{r} {cy}(interactive){r}",
            mg = c.mg,
            b = c.b,
            r = c.r,
            yl = c.yl,
            cy = c.cy,
            cmd = s.cmd_display,
        );
    } else {
        eprintln!(
            "{mg} │{r} {b}Command   :{r} {yl}{cmd}{r}",
            mg = c.mg,
            b = c.b,
            r = c.r,
            yl = c.yl,
            cmd = s.cmd_display,
        );
    }
    if !s.env_display.is_empty() {
        eprintln!(
            "{mg} │{r} {b}Env       :{r} {gr}{env}{r}",
            mg = c.mg,
            b = c.b,
            r = c.r,
            gr = c.gr,
            env = s.env_display,
        );
    }
    if let Some(user) = s.user {
        eprintln!(
            "{mg} │{r} {b}User      :{r} {gr}{user}{r}",
            mg = c.mg,
            b = c.b,
            r = c.r,
            gr = c.gr,
        );
    }
    eprintln!("{rd} {footer}{r}", rd = c.rd, r = c.r);
}

// ── Post-execution footer ─────────────────────────────────────────────────────

pub fn print_footer(exit_code: i32, elapsed_ms: u128, logfile: Option<&Path>) {
    let c = Colors::stderr();
    let ext = line_ext();
    let hdr_line = format!("──────────────────────────────────────────────────────────{ext}");
    let sep_line =
        format!("└───────────────────────────────────────────────────────────────────{ext}");

    let (status_color, status_label) = if exit_code == 0 {
        (c.gr, "✓ exit 0".to_string())
    } else {
        (c.rd, format!("✗ exit {exit_code}"))
    };

    let duration = fmt_duration(elapsed_ms);

    eprintln!("{rd} ┌─ result {hdr_line}{r}", rd = c.rd, r = c.r);
    eprintln!(
        "{mg} │{r} {b}Status   :{r} {sc}{status}{r}",
        mg = c.mg,
        b = c.b,
        r = c.r,
        sc = status_color,
        status = status_label,
    );
    eprintln!(
        "{mg} │{r} {b}Duration :{r} {dur}",
        mg = c.mg,
        b = c.b,
        r = c.r,
        dur = duration,
    );
    if let Some(log) = logfile {
        eprintln!(
            "{mg} │{r} {b}Log      :{r} {yl}{log}{r}",
            mg = c.mg,
            b = c.b,
            r = c.r,
            yl = c.yl,
            log = log.display(),
        );
    }
    eprintln!("{mg} {sep_line}{r}\n", mg = c.mg, r = c.r);
}

// ── Duration formatting ───────────────────────────────────────────────────────

pub fn fmt_duration(ms: u128) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{}.{}s", ms / 1000, (ms % 1000) / 100)
    } else {
        format!("{}m {}s", ms / 60_000, (ms % 60_000) / 1000)
    }
}

// ── Env display helper ────────────────────────────────────────────────────────

pub fn env_display(env_flags: &[String]) -> String {
    env_flags
        .chunks(2)
        .filter(|c| c.len() == 2 && c[0] == "-e")
        .map(|c| c[1].as_str())
        .collect::<Vec<_>>()
        .join(", ")
}
