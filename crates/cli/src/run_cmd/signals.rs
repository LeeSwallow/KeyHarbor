//! Passing stop signals sent to `keyharbor run` on to the child (J-83).

use anyhow::{Result, bail};

/// Signals asking `keyharbor run` to stop, which the child gets instead (J-83). SIGINT is not
/// here: Ctrl-C reaches the child from the terminal, as it is in our process group.
const FORWARDED: [libc::c_int; 2] = [libc::SIGTERM, libc::SIGHUP];

pub(super) fn forwarded_set() -> libc::sigset_t {
    // SAFETY: sigemptyset and sigaddset only write the set they are given.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        for signal in FORWARDED {
            libc::sigaddset(&mut set, signal);
        }
        set
    }
}

/// Blocks the forwarded signals in this thread (and the threads it starts after), so that
/// only [`forward_signals`] receives them.
pub(super) fn block_forwarded() -> Result<()> {
    let set = forwarded_set();
    // SAFETY: pthread_sigmask reads `set` and changes this thread's mask only.
    if unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()) } != 0 {
        bail!("could not block signals");
    }
    Ok(())
}

/// Passes every forwarded signal on to the child until this process exits.
pub(super) fn forward_signals(child: u32) {
    let Ok(pid) = libc::pid_t::try_from(child) else {
        return;
    };
    std::thread::spawn(move || {
        let set = forwarded_set();
        loop {
            let mut signal = 0;
            // SAFETY: sigwait reads `set` and writes `signal`; the signals are blocked.
            if unsafe { libc::sigwait(&set, &mut signal) } != 0 {
                return;
            }
            // SAFETY: kill only sends a signal.
            unsafe { libc::kill(pid, signal) };
        }
    });
}
