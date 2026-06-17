use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::time::Instant;

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

fn run_interactive(container: &str, step: &ExecStep) -> ExecResult {
    print_summary(step);

    let args = build_it_args(container, step);
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

fn run_background(container: &str, step: &ExecStep) -> ExecResult {
    print_summary(step);

    let (mut logfile, log_path) = match logfile::create() {
        Ok(t) => t,
        Err(e) => output::die(&e),
    };

    // write header to logfile
    {
        use std::io::Write;
        let _ = writeln!(logfile, "{}", "=".repeat(80));
        let _ = writeln!(logfile, "Command   : {}", step.display_cmd);
        let _ = writeln!(logfile, "Directory : {}", step.workdir);
        if !step.env_display.is_empty() {
            let _ = writeln!(logfile, "Env       : {}", step.env_display);
        }
        let _ = writeln!(logfile, "{}", "=".repeat(80));
    }

    let exec_args = build_exec_args(container, step);
    let is_tty = output::stdout_is_tty();
    let start = Instant::now();

    let mut child = match Command::new("docker")
        .args(&exec_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => output::die(&format!("docker exec: {e}")),
    };

    signal::DOCKER_PID.store(child.id() as i32, Ordering::SeqCst);
    signal::install();

    let stderr = child.stderr.take().unwrap();
    let stdout = child.stdout.take().unwrap();

    let cmd_for_kill = match &step.cmd {
        StepCmd::Alias(c) => c.clone(),
        StepCmd::Literal(v) => v.first().cloned().unwrap_or_default(),
    };
    let container_owned = container.to_owned();

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
    signal::DOCKER_PID.store(0, Ordering::SeqCst);

    if signal::KILLED.load(Ordering::SeqCst) {
        signal::kill_step(&container_owned, &cmd_for_kill);
        eprintln!();
        output::print_footer(130, elapsed, Some(&log_path));
        signal::reraise();
    }

    let show_log = if code != 0 {
        Some(log_path.as_path())
    } else {
        None
    };
    output::print_footer(code, elapsed, show_log);
    ExecResult { exit_code: code }
}

fn print_summary(step: &ExecStep) {
    output::print_summary(&SummaryInfo {
        profile: &step.profile,
        image: &step.image,
        container: &step.container,
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

fn build_it_args(container: &str, step: &ExecStep) -> Vec<String> {
    let mut args: Vec<String> = vec!["exec".into(), "-it".into()];
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
