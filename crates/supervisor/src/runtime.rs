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

/// How long `endpoint` waits for Chrome to write `DevToolsActivePort`. As long as a session
/// start may take (core `START_TIMEOUT`): on a busy machine Chrome writes the file seconds
/// after the session is running, and 5 s was too short on CI (#25).
const ENDPOINT_WAIT: Duration = Duration::from_secs(15);
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
    fn has_browser(&self, browser: &Path) -> bool {
        browser.is_file()
    }

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
        let deadline = Instant::now() + ENDPOINT_WAIT;
        loop {
            // Chrome creates the file before it writes the port, so a half-written file is
            // "not yet", not "invalid", until the deadline (#31 CI).
            let parsed = match std::fs::symlink_metadata(&file) {
                Ok(meta) if meta.is_file() && meta.len() <= 256 => {
                    parse_devtools_active_port(&std::fs::read_to_string(&file)?)
                }
                Ok(_) => bail!("invalid DevToolsActivePort file"),
                Err(_) => Err(anyhow::anyhow!("browser debugging endpoint is not ready")),
            };
            match parsed {
                Ok(endpoint) => return Ok(endpoint),
                Err(e) if Instant::now() >= deadline => return Err(e),
                Err(_) => thread::sleep(Duration::from_millis(50)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "11111111-1111-4111-8111-111111111111";
    const PORT_FILE: &str = "12345\n/devtools/browser/11111111-1111-4111-8111-111111111111\n";

    #[test]
    fn endpoint_waits_while_chrome_is_still_writing_the_port_file() {
        let data = tempfile::tempdir().unwrap();
        let id = SessionId::parse(ID).unwrap();
        let profile = paths::profile_dir(data.path(), &id);
        std::fs::create_dir_all(&profile).unwrap();
        let file = profile.join(DEVTOOLS_ACTIVE_PORT);
        // Chrome creates the file before it writes the port (#31 CI: "invalid debug port").
        std::fs::write(&file, "").unwrap();
        let writer = thread::spawn(move || {
            thread::sleep(Duration::from_millis(200));
            std::fs::write(&file, PORT_FILE).unwrap();
        });
        let runtime = ProcessRuntime::new(data.path(), Path::new("unused"));
        let endpoint = runtime.debug_endpoint(id).unwrap();
        writer.join().unwrap();
        assert_eq!(
            endpoint,
            "ws://127.0.0.1:12345/devtools/browser/11111111-1111-4111-8111-111111111111"
        );
    }
}
