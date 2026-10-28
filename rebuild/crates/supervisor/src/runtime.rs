//! `SessionRuntime` over OS processes: the supervisor lock file, the detached `__supervise`
//! process, its control socket and the `DevToolsActivePort` file of a `--cdp` browser.

use crate::{
    argv::SuperviseArgs,
    control::{
        client,
        wire::{ControlEnvelope, ControlRequest},
    },
    lock, paths,
};
use anyhow::{Result, bail};
use keyharbor_broker::chrome::{DEVTOOLS_ACTIVE_PORT, parse_devtools_active_port};
use keyharbor_domain::{
    Generation, SessionId,
    ports::{
        SessionRuntime,
        runtime::{LaunchOptions, RuntimeLock, StartedSupervisor},
    },
};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

/// Session processes under one data directory, started as `exe --data-dir … __supervise …`
/// ([`SuperviseArgs`]).
pub struct ProcessRuntime {
    data: PathBuf,
    exe: PathBuf,
}

impl ProcessRuntime {
    pub fn new(data: &Path, exe: &Path) -> Self {
        Self {
            data: data.to_path_buf(),
            exe: exe.to_path_buf(),
        }
    }
}

struct Supervisor(Child);
impl StartedSupervisor for Supervisor {
    fn exited(&mut self) -> Result<Option<String>> {
        Ok(self.0.try_wait()?.map(|status| status.to_string()))
    }
    fn detach(self: Box<Self>) {
        let mut child = self.0;
        thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

impl SessionRuntime for ProcessRuntime {
    fn is_alive(&self, id: SessionId) -> bool {
        lock::is_alive(&self.data, &id)
    }

    fn lock(&self, id: SessionId) -> Result<RuntimeLock> {
        Ok(Box::new(lock::acquire_lock(&paths::lock_path(
            &self.data, &id,
        ))?))
    }

    fn start(
        &self,
        id: SessionId,
        generation: Generation,
        options: &LaunchOptions<'_>,
    ) -> Result<Box<dyn StartedSupervisor>> {
        let args = SuperviseArgs {
            id,
            generation,
            browser: options.browser.to_path_buf(),
            url: options.url.map(str::to_owned),
            cdp: options.cdp,
        };
        let mut command = Command::new(&self.exe);
        command
            .args(args.to_argv(&self.data))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        Ok(Box::new(Supervisor(command.spawn()?)))
    }

    fn close(&self, id: SessionId) -> Result<()> {
        client::request(
            &self.data,
            &id,
            &ControlEnvelope::local(ControlRequest::Close),
        )?;
        Ok(())
    }

    fn remove_socket(&self, id: SessionId) {
        let _ = std::fs::remove_file(paths::socket_path(&self.data, &id));
    }

    fn debug_endpoint(&self, id: SessionId) -> Result<String> {
        let file = paths::profile_dir(&self.data, &id).join(DEVTOOLS_ACTIVE_PORT);
        let deadline = Instant::now() + Duration::from_secs(5);
        let text = loop {
            match std::fs::symlink_metadata(&file) {
                Ok(meta) if meta.is_file() && meta.len() <= 256 => {
                    break std::fs::read_to_string(&file)?;
                }
                Ok(_) => bail!("invalid DevToolsActivePort file"),
                Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
                Err(_) => bail!("browser debugging endpoint is not ready"),
            }
        };
        parse_devtools_active_port(&text)
    }
}
