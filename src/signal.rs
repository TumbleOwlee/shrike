use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use nix::sys::signal::{self, SigAction, SigHandler, Signal};

pub static KILLED: AtomicBool = AtomicBool::new(false);
pub static KILL_SIG: AtomicI32 = AtomicI32::new(0);
/// Raw fd of the PTY master. Set before `install()`, cleared after the child
/// exits. The signal handler writes the interrupt character (^C / 0x03) here
/// so the PTY relay delivers SIGINT to the entire container process group.
pub static PTY_MASTER_FD: AtomicI32 = AtomicI32::new(-1);

extern "C" fn handler(sig: libc::c_int) {
    KILLED.store(true, Ordering::SeqCst);
    KILL_SIG.store(sig, Ordering::SeqCst);
    let fd = PTY_MASTER_FD.load(Ordering::SeqCst);
    if fd >= 0 {
        const INTR: u8 = 0x03;
        unsafe {
            libc::write(fd, &INTR as *const u8 as *const libc::c_void, 1);
        }
    }
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

fn caught_signal() -> Signal {
    if KILL_SIG.load(Ordering::SeqCst) == libc::SIGTERM {
        Signal::SIGTERM
    } else {
        Signal::SIGINT
    }
}

pub fn reraise() {
    let sig = caught_signal();
    reset();
    let _ = signal::raise(sig);
}
