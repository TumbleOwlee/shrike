use std::io::{BufRead, BufReader, Write};
use std::process::Command;
use std::sync::atomic::Ordering;
use std::time::Instant;

use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize};
use terminal_size::{terminal_size, Height, Width};

use crate::display::output::{self, SummaryInfo};
use crate::display::rolling::RollingDisplay;
use crate::logfile;
use crate::signal;

pub struct ExecStep {
    pub cmd: StepCmd,
    pub workdir: String,
    pub display_cmd: String,
    pub env_flags: Vec<String>,
    pub env_display: String,
    pub user: Option<String>,
    pub interactive: bool,
    // summary display
    pub profile: String,
    pub image: String,
    pub container: String,
    pub ports: Vec<String>,
    pub volumes: Vec<String>,
}

pub enum StepCmd {
    Alias(String),
    Literal(Vec<String>),
}

pub struct ExecResult {
    pub exit_code: i32,
}

pub fn run(container: &str, step: &ExecStep) -> ExecResult {
    if step.interactive {
        run_interactive(container, step)
    } else {
        run_background(container, step)
    }
}

// ── PTY helpers ───────────────────────────────────────────────────────────────

fn pty_size() -> PtySize {
    if let Some((Width(cols), Height(rows))) = terminal_size() {
        PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }
    } else {
        PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 }
    }
}

fn open_pty() -> PtyPair {
    native_pty_system()
        .openpty(pty_size())
        .unwrap_or_else(|e| output::die(&format!("openpty: {e}")))
}

fn pty_cmd(args: &[String]) -> CommandBuilder {
    let mut cmd = CommandBuilder::new("docker");
    for arg in args {
        cmd.arg(arg);
    }
    cmd
}

// ── Interactive exec ──────────────────────────────────────────────────────────
// docker exec -it already allocates a container PTY and connects the user's
// terminal directly, so signal propagation works without a portable-pty layer.

fn run_interactive(container: &str, step: &ExecStep) -> ExecResult {
    print_summary(step);

    let args = build_exec_args(container, step);
    let start = Instant::now();

    let mut child = match Command::new("docker").args(&args).spawn() {
        Ok(c) => c,
        Err(e) => output::die(&format!("docker exec: {e}")),
    };

    signal::install();
    let status = child.wait().unwrap_or_else(|_| std::process::exit(1));
    let elapsed = start.elapsed().as_millis();
    let code = status.code().unwrap_or(1);

    if signal::KILLED.load(Ordering::SeqCst) {
        eprintln!();
        output::print_footer(130, elapsed, None);
        signal::reraise();
    }

    output::print_footer(code, elapsed, None);
    ExecResult { exit_code: code }
}

// ── Background exec ───────────────────────────────────────────────────────────

fn run_background(container: &str, step: &ExecStep) -> ExecResult {
    print_summary(step);

    let (mut logfile, log_path) = match logfile::create() {
        Ok(t) => t,
        Err(e) => output::die(&format!("failed to create logfile {e}")),
    };

    {
        let _ = writeln!(logfile, "{}", "=".repeat(80));
        let _ = writeln!(logfile, "Command   : {}", step.display_cmd);
        let _ = writeln!(logfile, "Directory : {}", step.workdir);
        if !step.env_display.is_empty() {
            let _ = writeln!(logfile, "Env       : {}", step.env_display);
        }
        let _ = writeln!(logfile, "{}", "=".repeat(80));
    }

    // -it allocates a container PTY (and attaches stdin so docker exec reads
    // from the PTY slave). The signal handler writes the interrupt character
    // (0x03) to the PTY master, which docker exec relays to the container PTY,
    // delivering SIGINT to the entire container process group.
    let exec_args = build_exec_args(container, step);
    let is_tty = output::stdout_is_tty();
    let start = Instant::now();

    let pair = open_pty();
    let master_fd = pair.master.as_raw_fd().unwrap_or(-1);

    let slave = pair.slave;
    let mut child = slave
        .spawn_command(pty_cmd(&exec_args))
        .unwrap_or_else(|e| output::die(&format!("docker exec: {e}")));
    drop(slave);

    let pty_reader = pair
        .master
        .try_clone_reader()
        .unwrap_or_else(|e| output::die(&format!("pty reader: {e}")));

    signal::PTY_MASTER_FD.store(master_fd, Ordering::SeqCst);
    signal::install();

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(pty_reader).lines().map_while(Result::ok) {
            // PTY output uses \r\n; strip the trailing \r before display/log
            let line = line.trim_end_matches('\r').to_owned();
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    let mut display = RollingDisplay::new(is_tty);
    for line in rx {
        if signal::KILLED.load(Ordering::SeqCst) {
            break;
        }
        display.feed(&line, &mut logfile);
    }

    // Keep _master alive through wait() so closing it doesn't race with the child.
    let _master = pair.master;
    signal::PTY_MASTER_FD.store(-1, Ordering::SeqCst);
    let status = child.wait().unwrap_or_else(|_| std::process::exit(1));
    let elapsed = start.elapsed().as_millis();
    let code = status.exit_code() as i32;

    if signal::KILLED.load(Ordering::SeqCst) {
        eprintln!();
        output::print_footer(130, elapsed, Some(&log_path));
        signal::reraise();
    }

    let show_log = if code != 0 { Some(log_path.as_path()) } else { None };
    output::print_footer(code, elapsed, show_log);
    ExecResult { exit_code: code }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

fn print_summary(step: &ExecStep) {
    output::print_summary(&SummaryInfo {
        profile: &step.profile,
        image: Some(&step.image),
        container: Some(&step.container),
        ports: &step.ports,
        volumes: &step.volumes,
        workdir: &step.workdir,
        cmd_display: &step.display_cmd,
        env_display: &step.env_display,
        user: step.user.as_deref(),
        interactive: step.interactive,
    });
}

fn build_exec_args(container: &str, step: &ExecStep) -> Vec<String> {
    let mut args: Vec<String> = vec!["exec".into()];
    args.push("-it".into());
    if let Some(ref user) = step.user {
        args.push("--user".into());
        args.push(user.clone());
    }
    args.push("-w".into());
    args.push(step.workdir.clone());
    args.extend_from_slice(&step.env_flags);
    args.push(container.into());
    match &step.cmd {
        StepCmd::Alias(cmd) => {
            args.push("sh".into());
            args.push("-c".into());
            args.push(cmd.clone());
        }
        StepCmd::Literal(cmd) => {
            args.extend(cmd.iter().cloned());
        }
    }
    args
}

// ── Native (direct-mode) exec ─────────────────────────────────────────────────

pub fn run_native(
    mut cmd: std::process::Command,
    display_cmd: &str,
    workdir: &str,
    profile: &str,
    env_disp: &str,
    interactive: bool,
) -> ExecResult {
    use std::process::Stdio;

    output::print_summary(&SummaryInfo {
        profile,
        image: None,
        container: None,
        ports: &[],
        volumes: &[],
        workdir,
        cmd_display: display_cmd,
        env_display: env_disp,
        user: None,
        interactive,
    });

    let start = std::time::Instant::now();

    if interactive {
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => output::die(&format!("exec: {e}")),
        };
        signal::install();
        let status = child.wait().unwrap_or_else(|_| std::process::exit(1));
        let elapsed = start.elapsed().as_millis();
        let code = status.code().unwrap_or(1);
        if signal::KILLED.load(Ordering::SeqCst) {
            eprintln!();
            output::print_footer(130, elapsed, None);
            signal::reraise();
        }
        output::print_footer(code, elapsed, None);
        return ExecResult { exit_code: code };
    }

    let (mut logfile, log_path) = match logfile::create() {
        Ok(t) => t,
        Err(e) => output::die(&format!("failed to create logfile {e}")),
    };

    {
        let _ = writeln!(logfile, "{}", "=".repeat(80));
        let _ = writeln!(logfile, "Command   : {display_cmd}");
        let _ = writeln!(logfile, "Directory : {workdir}");
        if !env_disp.is_empty() {
            let _ = writeln!(logfile, "Env       : {env_disp}");
        }
        let _ = writeln!(logfile, "{}", "=".repeat(80));
    }

    let is_tty = output::stdout_is_tty();

    let mut child = match cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
        Ok(c) => c,
        Err(e) => output::die(&format!("exec: {e}")),
    };

    signal::install();

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let tx2 = tx.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let _ = tx2.send(line);
        }
    });

    let mut display = RollingDisplay::new(is_tty);
    for line in rx {
        if signal::KILLED.load(Ordering::SeqCst) {
            break;
        }
        display.feed(&line, &mut logfile);
    }

    let status = child.wait().unwrap_or_else(|_| std::process::exit(1));
    let elapsed = start.elapsed().as_millis();
    let code = status.code().unwrap_or(1);

    if signal::KILLED.load(Ordering::SeqCst) {
        eprintln!();
        output::print_footer(130, elapsed, Some(&log_path));
        signal::reraise();
    }

    let show_log = if code != 0 { Some(log_path.as_path()) } else { None };
    output::print_footer(code, elapsed, show_log);
    ExecResult { exit_code: code }
}
