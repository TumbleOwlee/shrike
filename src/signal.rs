use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use nix::sys::signal::{self, SigAction, SigHandler, Signal};

pub static KILLED: AtomicBool = AtomicBool::new(false);
pub static DOCKER_PID: AtomicI32 = AtomicI32::new(0);
pub static KILL_SIG: AtomicI32 = AtomicI32::new(0);

extern "C" fn handler(sig: libc::c_int) {
    KILLED.store(true, Ordering::SeqCst);
    KILL_SIG.store(sig, Ordering::SeqCst);
}

pub fn install() {
    let action = SigAction::new(
        SigHandler::Handler(handler),
        signal::SaFlags::empty(),
        signal::SigSet::empty(),
    );
    unsafe {
        signal::sigaction(Signal::SIGINT, &action).unwrap();
        signal::sigaction(Signal::SIGTERM, &action).unwrap();
    }
}

pub fn reset() {
    let default = SigAction::new(
        SigHandler::SigDfl,
        signal::SaFlags::empty(),
        signal::SigSet::empty(),
    );
    unsafe {
        signal::sigaction(Signal::SIGINT, &default).unwrap();
        signal::sigaction(Signal::SIGTERM, &default).unwrap();
    }
}

/// Kill the host-side docker exec process and the container-side command.
/// `exec_argv` is the exact argument line the container runs (e.g. `sh -c make`),
/// matched against `ps` output so unrelated container processes are left alone.
pub fn kill_step(container: &str, exec_argv: &str) {
    let pid = DOCKER_PID.load(Ordering::SeqCst);
    let sig = caught_signal();

    // kill host docker exec process
    if pid > 0 {
        let _ = signal::kill(nix::unistd::Pid::from_raw(pid), sig);
    }

    // kill container-side process
    kill_container_side(container, exec_argv);
}

fn caught_signal() -> Signal {
    if KILL_SIG.load(Ordering::SeqCst) == libc::SIGTERM {
        Signal::SIGTERM
    } else {
        Signal::SIGINT
    }
}

fn kill_container_side(container: &str, exec_argv: &str) {
    let Ok(ps_out) = Command::new("docker")
        .args(["exec", container, "ps", "ax", "-o", "pid,args"])
        .output()
    else {
        return;
    };

    let sig_flag = match caught_signal() {
        Signal::SIGTERM => "-TERM",
        _ => "-INT",
    };

    let stdout = String::from_utf8_lossy(&ps_out.stdout);
    for line in stdout.lines() {
        // ps -o pid,args → "  123 sh -c make build"; split off the pid, then
        // require the remaining argv to match exactly (not a loose substring).
        let line = line.trim();
        let Some((pid_str, argv)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        if argv.trim() == exec_argv && pid_str.chars().all(|c| c.is_ascii_digit()) {
            let _ = Command::new("docker")
                .args(["exec", container, "kill", sig_flag, pid_str])
                .output();
        }
    }
}

/// Re-raise the caught signal to the parent shell.
pub fn reraise() {
    let sig = caught_signal();
    reset();
    let _ = signal::raise(sig);
}
