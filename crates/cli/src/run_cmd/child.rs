//! Starting the child: its environment, arguments and limits.

use super::signals::forwarded_set;
use anyhow::{Context, Result};
use std::{
    ffi::OsString,
    path::Path,
    process::{Child, Command, Stdio},
};
use zeroize::Zeroizing;

/// Starts `program` with `args`, `vars` and no core dumps, its output piped back to us. The
/// `Command` holding the values is dropped before this returns.
pub(super) fn spawn(
    program: &OsString,
    args: &[Zeroizing<String>],
    vars: &[(String, Zeroizing<String>)],
) -> Result<Child> {
    let mut command = Command::new(program);
    command
        .args(args.iter().map(|a| a.as_str()))
        .envs(vars.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let set = forwarded_set();
    // SAFETY: pthread_sigmask and setrlimit are async-signal-safe; the set is built before fork.
    unsafe {
        use std::os::unix::process::CommandExt;
        command.pre_exec(move || {
            if libc::pthread_sigmask(libc::SIG_UNBLOCK, &set, std::ptr::null_mut()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            let none = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if libc::setrlimit(libc::RLIMIT_CORE, &none) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command
        .spawn()
        .with_context(|| format!("could not start {}", Path::new(program).display()))
}
