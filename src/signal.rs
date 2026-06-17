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
pub fn kill_step(container: &str, exec_cmd: &str) {
    let pid = DOCKER_PID.load(Ordering::SeqCst);
    let sig_num = KILL_SIG.load(Ordering::SeqCst);
    let sig = if sig_num == libc::SIGTERM {
        Signal::SIGTERM
    } else {
        Signal::SIGINT
    };

    // kill host docker exec process
    if pid > 0 {
        let _ = signal::kill(nix::unistd::Pid::from_raw(pid), sig);
    }

    // kill container-side process
    kill_container_side(container, exec_cmd);
}

fn kill_container_side(container: &str, exec_cmd: &str) {
    let Ok(ps_out) = Command::new("docker")
        .args(["exec", container, "ps", "ax", "-o", "pid,args"])
        .output()
    else {
        return;
    };

    let stdout = String::from_utf8_lossy(&ps_out.stdout);
    for line in stdout.lines() {
        if line.contains(exec_cmd) && !line.contains("ps ax") {
            let pid_str = line.trim().split_whitespace().next().unwrap_or("");
            if !pid_str.is_empty() {
                let _ = Command::new("docker")
                    .args(["exec", container, "kill", "-TERM", pid_str])
                    .output();
            }
        }
    }
}

/// Re-raise the caught signal to the parent shell.
pub fn reraise() {
    let sig_num = KILL_SIG.load(Ordering::SeqCst);
    let sig = if sig_num == libc::SIGTERM {
        Signal::SIGTERM
    } else {
        Signal::SIGINT
    };
    reset();
    let _ = signal::raise(sig);
}
