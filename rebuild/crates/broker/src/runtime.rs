//! `SessionRuntime` over OS processes: the supervisor lock file, the detached `__supervise`
//! process, its control socket and the `DevToolsActivePort` file of a `--cdp` browser.

use crate::{control, paths, supervisor};
use anyhow::{Context, Result, bail};
use keyharbor_domain::{
    Generation, SessionId,
    ports::{
        SessionRuntime,
        runtime::{LaunchOptions, RuntimeLock, StartedSupervisor},
    },
};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

/// The browser `open` uses when none is given.
pub const DEFAULT_BROWSER: &str = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

/// Session processes under one data directory, started as `exe --data-dir … __supervise …`.
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
        supervisor::is_alive(&self.data, &id)
    }

    fn lock(&self, id: SessionId) -> Result<RuntimeLock> {
        Ok(Box::new(supervisor::acquire_lock(&paths::lock_path(
            &self.data, &id,
        ))?))
    }

    fn start(
        &self,
        id: SessionId,
        generation: Generation,
        options: &LaunchOptions<'_>,
    ) -> Result<Box<dyn StartedSupervisor>> {
        let mut command = Command::new(&self.exe);
        command
            .arg("--data-dir")
            .arg(&self.data)
            .arg("__supervise")
            .arg(id.to_string())
            .arg(generation.to_string())
            .arg(options.browser);
        if let Some(url) = options.url {
            command.arg("--url").arg(url);
        }
        if options.cdp {
            command.arg("--cdp");
        }
        command
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
        control::call(&self.data, &id, &json!({"op": "close"}))?;
        Ok(())
    }

    fn remove_socket(&self, id: SessionId) {
        let _ = std::fs::remove_file(paths::socket_path(&self.data, &id));
    }

    fn debug_endpoint(&self, id: SessionId) -> Result<String> {
        let file = paths::profile_dir(&self.data, &id).join("DevToolsActivePort");
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
        let mut lines = text.lines();
        let port: u16 = lines
            .next()
            .and_then(|p| p.parse().ok())
            .filter(|p| *p != 0)
            .context("invalid debug port")?;
        let path = lines.next().context("missing browser endpoint")?;
        let browser = path
            .strip_prefix("/devtools/browser/")
            .context("invalid browser endpoint")?;
        uuid_like(browser)?;
        if lines.next().is_some() {
            bail!("invalid browser endpoint");
        }
        Ok(format!("ws://127.0.0.1:{port}{path}"))
    }
}

fn uuid_like(value: &str) -> Result<()> {
    let ok = value.len() == 36
        && value.char_indices().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        });
    if !ok {
        bail!("invalid browser endpoint");
    }
    Ok(())
}
