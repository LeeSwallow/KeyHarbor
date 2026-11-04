//! Passing stop signals sent to `keyharbor run` on to the child (J-83), and outliving the
//! child so that the run's files and lookup socket are removed however it ends (J-122).

use anyhow::{Result, bail};

/// Signals asking `keyharbor run` to stop, which the child gets instead (J-83).
const FORWARDED: [libc::c_int; 2] = [libc::SIGTERM, libc::SIGHUP];
/// Ctrl-C reaches the child from the terminal, as it is in our process group. `run` only
/// takes it out of the way so it is not killed before the child ends and it can clean up.
const SWALLOWED: [libc::c_int; 1] = [libc::SIGINT];

/// Every signal `run` blocks and waits for itself; the child unblocks them all.
pub(super) fn blocked_set() -> libc::sigset_t {
    // SAFETY: sigemptyset and sigaddset only write the set they are given.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        for signal in FORWARDED.into_iter().chain(SWALLOWED) {
            libc::sigaddset(&mut set, signal);
        }
        set
    }
}

/// Blocks the signals in this thread (and the threads it starts after), so that only
/// [`forward_signals`] receives them.
pub(super) fn block_forwarded() -> Result<()> {
    let set = blocked_set();
    // SAFETY: pthread_sigmask reads `set` and changes this thread's mask only.
    if unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()) } != 0 {
        bail!("could not block signals");
    }
    Ok(())
}

/// Passes every forwarded signal on to the child, and drops the swallowed ones, until this
/// process exits.
pub(super) fn forward_signals(child: u32) {
    let Ok(pid) = libc::pid_t::try_from(child) else {
        return;
    };
    std::thread::spawn(move || {
        let set = blocked_set();
        loop {
            let mut signal = 0;
            // SAFETY: sigwait reads `set` and writes `signal`; the signals are blocked.
            if unsafe { libc::sigwait(&set, &mut signal) } != 0 {
                return;
            }
            if FORWARDED.contains(&signal) {
                // SAFETY: kill only sends a signal.
                unsafe { libc::kill(pid, signal) };
            }
        }
    });
}
