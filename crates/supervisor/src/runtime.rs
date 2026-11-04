//! `SessionRuntime` over OS processes: the supervisor lock file, the detached `__supervise`
//! process, its control socket and the `DevToolsActivePort` file of a `--cdp` browser.

use crate::{
    argv::SuperviseArgs,
    control::{
        client,
        wire::{ControlEnvelope, ControlRequest, StatusResultWire},
    },
    lock, paths,
};
use anyhow::{Result, bail};
use keyharbor_broker::chrome::{DEVTOOLS_ACTIVE_PORT, parse_devtools_active_port};
use keyharbor_broker::{chrome::locate as locate_chrome, ego::probe};
use keyharbor_domain::{
    Generation, SessionId,
    browser_choice::{BrowserChoice, BrowserKind},
    ports::{
        SessionRuntime, SettingsStore,
        runtime::{LaunchOptions, ResolvedBrowser, RuntimeLock, StartedSupervisor},
    },
    session::substitution_lock::LockReason,
    settings::{BROWSER_CALL_SECONDS, BROWSER_ENDPOINT_SECONDS},
};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

/// How long the browser gets, from the settings the composition root reads (#201).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BrowserWaits {
    /// How long `endpoint` waits for Chrome to write `DevToolsActivePort`
    /// (`browser_endpoint_seconds`). On a busy machine Chrome writes it seconds after the
    /// session is running; 5 s was too short on CI (#25).
    pub endpoint: Duration,
    /// How long one CDP call may go unanswered (`browser_call_seconds`), handed to each new
    /// supervisor as `--call-seconds`.
    pub call: Duration,
}

impl BrowserWaits {
    /// The two settings' values, or their defaults when never set.
    pub fn load(settings: &dyn SettingsStore) -> Result<Self> {
        use keyharbor_core::settings::seconds;
        Ok(Self {
            endpoint: seconds(settings, BROWSER_ENDPOINT_SECONDS)?,
            call: seconds(settings, BROWSER_CALL_SECONDS)?,
        })
    }
}

/// Session processes under one data directory, started as `exe --data-dir … __supervise …`
/// ([`SuperviseArgs`]).
pub struct ProcessRuntime {
    data: PathBuf,
    exe: PathBuf,
    waits: BrowserWaits,
}

impl ProcessRuntime {
    pub fn new(data: &Path, exe: &Path, waits: BrowserWaits) -> Self {
        Self {
            data: data.to_path_buf(),
            exe: exe.to_path_buf(),
            waits,
        }
    }

    /// The supervisor's arguments for one run.
    fn supervise_args(
        &self,
        id: SessionId,
        generation: Generation,
        options: &LaunchOptions<'_>,
    ) -> SuperviseArgs {
        SuperviseArgs {
            id,
            generation,
            browser: options.browser.executable.clone(),
            kind: options.browser.kind,
            headless: options.headless,
            url: options.url.map(str::to_owned),
            cdp: options.cdp,
            call_timeout: self.waits.call,
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

    fn resolve_browser(&self, choice: &BrowserChoice, wait: Duration) -> Result<ResolvedBrowser> {
        let chrome = |path: &Path| ResolvedBrowser {
            kind: BrowserKind::Chrome,
            executable: path.to_path_buf(),
        };
        let ego = |command: PathBuf| ResolvedBrowser {
            kind: BrowserKind::EgoLite,
            executable: command,
        };
        let installed_chrome = || {
            locate_chrome().map(|path| chrome(&path)).ok_or_else(|| {
                anyhow::anyhow!(
                    "Chrome was not found in /Applications, ~/Applications or PATH; set its path with `keyharbor settings set browser <path>`"
                )
            })
        };
        Ok(match choice {
            BrowserChoice::Chrome => installed_chrome()?,
            BrowserChoice::Path(path) => chrome(path),
            BrowserChoice::EgoLite => match probe::find(wait) {
                Some(command) => ego(command),
                None => bail!(
                    "ego lite is not available: `{}` was not found or did not answer within {}s",
                    probe::EGO_COMMAND,
                    wait.as_secs()
                ),
            },
            BrowserChoice::Auto => match probe::find(wait) {
                Some(command) => ego(command),
                None => installed_chrome()?,
            },
        })
    }

    fn preflight(&self, id: SessionId) -> Result<()> {
        paths::check_socket_path(&self.data, &id)
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
        let args = self.supervise_args(id, generation, options);
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

    fn lock_reason(&self, id: SessionId) -> Result<Option<LockReason>> {
        let result = client::request(
            &self.data,
            &id,
            &ControlEnvelope::local(ControlRequest::Status),
        )?;
        let status: StatusResultWire = serde_json::from_value(result)?;
        Ok(status.locked.as_deref().and_then(LockReason::parse))
    }

    /// Sent as the local user: only the person's interfaces (CLI after `TtyConfirm`, the admin
    /// app) call this; the MCP server's requests name its agent and are refused (#190).
    fn unlock(&self, id: SessionId) -> Result<()> {
        client::request(
            &self.data,
            &id,
            &ControlEnvelope::local(ControlRequest::Unlock),
        )?;
        Ok(())
    }

    fn remove_socket(&self, id: SessionId) {
        let _ = std::fs::remove_file(paths::socket_path(&self.data, &id));
    }

    fn remove_profile(&self, id: SessionId) -> Result<()> {
        let profile = paths::profile_dir(&self.data, &id);
        match std::fs::symlink_metadata(&profile) {
            Ok(meta) if meta.is_dir() => Ok(std::fs::remove_dir_all(&profile)?),
            Ok(_) => bail!("{} is not a directory", profile.display()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn debug_endpoint(&self, id: SessionId) -> Result<String> {
        let file = paths::profile_dir(&self.data, &id).join(DEVTOOLS_ACTIVE_PORT);
        let deadline = Instant::now() + self.waits.endpoint;
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

    /// Settings kept in memory.
    #[derive(Default)]
    struct Settings(std::sync::Mutex<std::collections::HashMap<String, String>>);
    impl SettingsStore for Settings {
        fn setting_get(&self, key: &str) -> Result<Option<String>> {
            Ok(self.0.lock().unwrap().get(key).cloned())
        }
        fn setting_set(&self, key: &str, value: &str) -> Result<()> {
            self.0.lock().unwrap().insert(key.into(), value.into());
            Ok(())
        }
    }

    fn defaults() -> BrowserWaits {
        BrowserWaits::load(&Settings::default()).unwrap()
    }

    #[test]
    fn the_waits_default_to_the_old_constants() {
        assert_eq!(
            defaults(),
            BrowserWaits {
                endpoint: Duration::from_secs(15),
                call: Duration::from_secs(30),
            }
        );
    }

    #[test]
    fn the_call_limit_reaches_the_supervisor_as_an_argument() {
        let settings = Settings::default();
        settings.setting_set(BROWSER_CALL_SECONDS, "75").unwrap();
        let runtime = ProcessRuntime::new(
            Path::new("/d"),
            Path::new("/kh"),
            BrowserWaits::load(&settings).unwrap(),
        );
        let browser = ResolvedBrowser {
            kind: BrowserKind::Chrome,
            executable: "/bin/chrome".into(),
        };
        let options = LaunchOptions {
            browser: &browser,
            url: None,
            cdp: false,
            headless: true,
        };
        let id = SessionId::parse(ID).unwrap();
        let generation = keyharbor_crypto::random::id::<Generation>();
        let argv = runtime
            .supervise_args(id, generation, &options)
            .to_argv(Path::new("/d"));
        let parsed = SuperviseArgs::parse(&argv[3..]).unwrap();
        assert_eq!(parsed.call_timeout, Duration::from_secs(75));
    }

    #[test]
    fn endpoint_gives_up_after_the_endpoint_setting() {
        let data = tempfile::tempdir().unwrap();
        let settings = Settings::default();
        settings.setting_set(BROWSER_ENDPOINT_SECONDS, "5").unwrap();
        let runtime = ProcessRuntime::new(
            data.path(),
            Path::new("unused"),
            BrowserWaits::load(&settings).unwrap(),
        );
        let started = Instant::now();
        assert!(
            runtime
                .debug_endpoint(SessionId::parse(ID).unwrap())
                .is_err()
        );
        let waited = started.elapsed();
        assert!(
            waited >= Duration::from_secs(5) && waited < Duration::from_secs(10),
            "{waited:?}"
        );
    }
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
        let runtime = ProcessRuntime::new(data.path(), Path::new("unused"), defaults());
        let endpoint = runtime.debug_endpoint(id).unwrap();
        writer.join().unwrap();
        assert_eq!(
            endpoint,
            "ws://127.0.0.1:12345/devtools/browser/11111111-1111-4111-8111-111111111111"
        );
    }
}
