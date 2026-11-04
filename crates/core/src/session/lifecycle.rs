//! Opening, stopping and recovering a session's supervisor, and reading its raw CDP endpoint.

use super::{RECOVER, active, note, require_supervisor, session, supervision};
use anyhow::{Context, Result, anyhow, bail};
use keyharbor_domain::{
    Generation, SessionId, State,
    audit::AuditKind,
    browser_choice::BrowserChoice,
    ports::{
        Clock, SessionRuntime, SettingsStore, Store,
        runtime::{LaunchOptions, ResolvedBrowser, StartedSupervisor},
    },
    settings::{BROWSER, EGO_PROBE_SECONDS, SESSION_START_SECONDS, SESSION_STOP_SECONDS},
};
use std::time::Duration;

/// How often `open` and `stop` look at the session while they wait.
const POLL: Duration = Duration::from_millis(50);

pub struct OpenOptions<'a> {
    pub url: Option<&'a str>,
    pub cdp: bool,
    /// From [`resolve_browser`].
    pub browser: &'a ResolvedBrowser,
    /// No window (Chrome only).
    pub headless: bool,
    /// Who asked, for the usage record (`cli` for the local user).
    pub agent: &'a str,
}

/// The browser to open in: `choice`, or the `browser` setting when none is given (#120).
pub fn resolve_browser(
    settings: &dyn SettingsStore,
    runtime: &dyn SessionRuntime,
    choice: Option<&BrowserChoice>,
) -> Result<ResolvedBrowser> {
    let choice = match choice {
        Some(choice) => choice.clone(),
        None => BrowserChoice::parse(&crate::settings::get(settings, BROWSER)?)
            .map_err(|e| anyhow::anyhow!("{BROWSER} {e}"))?,
    };
    runtime.resolve_browser(&choice, ego_probe(settings)?)
}

/// Whether ego lite answers within the `ego_probe_seconds` setting, the same wait `auto` gives
/// it when a session opens (the settings screen's "지금 ego lite가 켜져 있습니다").
pub fn ego_lite_answers(
    settings: &dyn SettingsStore,
    runtime: &dyn SessionRuntime,
) -> Result<bool> {
    let probe = ego_probe(settings)?;
    Ok(runtime
        .resolve_browser(&BrowserChoice::EgoLite, probe)
        .is_ok())
}

fn ego_probe(settings: &dyn SettingsStore) -> Result<Duration> {
    crate::settings::seconds(settings, EGO_PROBE_SECONDS)
}

/// Reserves the session and starts its supervisor, returning once the session is `running`
/// or failing after the `session_start_seconds` setting.
pub fn open(
    store: &dyn Store,
    runtime: &dyn SessionRuntime,
    clock: &dyn Clock,
    id: SessionId,
    options: &OpenOptions<'_>,
) -> Result<()> {
    let url = options
        .url
        .map(keyharbor_domain::validate_start_url)
        .transpose()?;
    // Read before the reservation, so an odd stored value leaves the session as it was.
    let start_wait = crate::settings::seconds(store, SESSION_START_SECONDS)?;
    let generation = reserve_run(store, runtime, id, options)?;
    let launch = LaunchOptions {
        browser: options.browser,
        url: url.as_deref(),
        cdp: options.cdp,
        headless: options.headless,
    };
    let mut child = match runtime.start(id, generation, &launch) {
        Ok(child) => child,
        Err(error) => {
            let error = error.context("start session supervisor");
            supervision::mark_failed(store, id, generation, &error);
            return Err(error);
        }
    };
    let result = wait_until_running(
        store,
        clock,
        id,
        generation,
        child.as_mut(),
        clock.now() + start_wait,
    );
    // Reap the supervisor when it eventually exits, so a long-lived caller leaves no zombie.
    child.detach();
    if result.is_ok() {
        note(
            store,
            options.agent,
            AuditKind::SessionOpened,
            id,
            &format!("running {}", options.browser.kind),
        );
    }
    result
}

/// Checks that the run can start and reserves it under the supervisor lock, returning the
/// run's generation. The lock is released before the supervisor takes it.
fn reserve_run(
    store: &dyn Store,
    runtime: &dyn SessionRuntime,
    id: SessionId,
    options: &OpenOptions<'_>,
) -> Result<Generation> {
    if !runtime.has_browser(&options.browser.executable) {
        bail!(
            "browser executable not found: {}",
            options.browser.executable.display()
        );
    }
    runtime.preflight(id)?;
    let row = session(store, id)?;
    let _lock = runtime.lock(id).context("session is already open")?;
    let Some(generation) = store.reserve(id, options.cdp, options.browser.kind)? else {
        if active(row.state) || row.state == State::Failed {
            bail!(
                "session is {} without a supervisor; close its browser, then {RECOVER}",
                row.state.as_str()
            );
        }
        bail!("session is {}", row.state.as_str());
    };
    Ok(generation)
}

/// Looks at the session every [`POLL`] until run `generation` reports `running` (or awaits the
/// user), fails, loses its reservation, its supervisor exits, or `deadline` passes.
fn wait_until_running(
    store: &dyn Store,
    clock: &dyn Clock,
    id: SessionId,
    generation: Generation,
    child: &mut dyn StartedSupervisor,
    deadline: Duration,
) -> Result<()> {
    loop {
        let row = session(store, id)?;
        if row.generation != Some(generation) {
            bail!("session reservation changed during startup");
        }
        match row.state {
            State::Running | State::AwaitingUser => return Ok(()),
            State::Starting => (),
            other => {
                let why = row
                    .failure
                    .as_deref()
                    .map(|r| format!(": {r}"))
                    .unwrap_or_default();
                bail!(
                    "session did not start ({}{why}); {RECOVER} before retrying",
                    other.as_str()
                );
            }
        }
        if let Some(status) = child.exited()? {
            let error = anyhow!("session supervisor exited during startup ({status})");
            supervision::mark_failed(store, id, generation, &error);
            bail!("{error}; {RECOVER} before retrying");
        }
        if clock.now() >= deadline {
            bail!("supervisor did not report ready; inspect the session with list before retrying");
        }
        clock.sleep(POLL);
    }
}

/// Asks the supervisor to close the browser and waits until the session has stopped, for at
/// most the `session_stop_seconds` setting.
pub fn stop(
    store: &dyn Store,
    runtime: &dyn SessionRuntime,
    clock: &dyn Clock,
    id: SessionId,
    agent: &str,
) -> Result<State> {
    let row = session(store, id)?;
    if !matches!(
        row.state,
        State::Running | State::AwaitingUser | State::Stopping
    ) {
        bail!("session cannot be stopped from {}", row.state.as_str());
    }
    require_supervisor(runtime, id)?;
    let generation = row.generation.context("session has no generation")?;
    let stop_wait = crate::settings::seconds(store, SESSION_STOP_SECONDS)?;
    if row.state != State::Stopping
        && !store.transition(id, generation, row.state, State::Stopping)?
    {
        bail!("session state changed; retry");
    }
    runtime.close(id)?;
    let deadline = clock.now() + stop_wait;
    loop {
        let row = session(store, id)?;
        if row.generation != Some(generation) || row.state != State::Stopping {
            note(
                store,
                agent,
                AuditKind::SessionStopped,
                id,
                row.state.as_str(),
            );
            return Ok(row.state);
        }
        if clock.now() >= deadline {
            bail!("browser did not exit in time; check the session with list");
        }
        clock.sleep(POLL);
    }
}

/// Returns an abandoned session to `stopped`. Refused while a supervisor holds the lock.
pub fn recover(store: &dyn Store, runtime: &dyn SessionRuntime, id: SessionId) -> Result<()> {
    session(store, id)?;
    let _lock = runtime
        .lock(id)
        .context("session supervisor is running; use stop instead")?;
    if !store.force_stopped(id)? {
        bail!("session is already stopped");
    }
    runtime.remove_socket(id);
    Ok(())
}

/// The raw CDP WebSocket address of a running session opened with `cdp`. Withheld while the
/// session is awaiting the user, as the prototype did.
pub fn endpoint(store: &dyn Store, runtime: &dyn SessionRuntime, id: SessionId) -> Result<String> {
    let row = session(store, id)?;
    if !row.cdp {
        bail!("raw CDP is not enabled for this session; open it with --cdp");
    }
    if row.state != State::Running {
        bail!("session is {}, not running", row.state.as_str());
    }
    require_supervisor(runtime, id)?;
    let endpoint = runtime.debug_endpoint(id)?;
    let latest = session(store, id)?;
    if latest.state != State::Running || latest.generation != row.generation {
        bail!("session changed while reading the endpoint");
    }
    Ok(endpoint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyharbor_domain::{
        browser_choice::BrowserKind,
        ports::{
            ManualClock, SessionStore,
            runtime::{LaunchOptions, RuntimeLock, StartedSupervisor},
        },
        session::substitution_lock::LockReason,
    };
    use std::{cell::RefCell, path::Path, sync::Mutex};

    #[derive(Default)]
    struct Settings(RefCell<Option<String>>);
    impl SettingsStore for Settings {
        fn setting_get(&self, key: &str) -> Result<Option<String>> {
            assert_eq!(key, EGO_PROBE_SECONDS);
            Ok(self.0.borrow().clone())
        }
        fn setting_set(&self, _: &str, value: &str) -> Result<()> {
            *self.0.borrow_mut() = Some(value.to_owned());
            Ok(())
        }
    }

    /// Ego lite answers only when given at least `answers_within`; records each wait.
    struct Probe {
        answers_within: Duration,
        waits: Mutex<Vec<Duration>>,
    }
    impl SessionRuntime for Probe {
        fn resolve_browser(
            &self,
            choice: &BrowserChoice,
            probe: Duration,
        ) -> Result<ResolvedBrowser> {
            assert_eq!(choice, &BrowserChoice::EgoLite);
            self.waits.lock().unwrap().push(probe);
            if probe < self.answers_within {
                bail!("ego lite did not answer");
            }
            Ok(ResolvedBrowser {
                kind: BrowserKind::EgoLite,
                executable: "ego-browser".into(),
            })
        }
        fn has_browser(&self, _: &Path) -> bool {
            unreachable!()
        }
        fn preflight(&self, _: SessionId) -> Result<()> {
            unreachable!()
        }
        fn is_alive(&self, _: SessionId) -> bool {
            unreachable!()
        }
        fn lock(&self, _: SessionId) -> Result<RuntimeLock> {
            unreachable!()
        }
        fn start(
            &self,
            _: SessionId,
            _: Generation,
            _: &LaunchOptions<'_>,
        ) -> Result<Box<dyn StartedSupervisor>> {
            unreachable!()
        }
        fn close(&self, _: SessionId) -> Result<()> {
            unreachable!()
        }
        fn lock_reason(&self, _: SessionId) -> Result<Option<LockReason>> {
            unreachable!()
        }
        fn unlock(&self, _: SessionId) -> Result<()> {
            unreachable!()
        }
        fn remove_socket(&self, _: SessionId) {
            unreachable!()
        }
        fn remove_profile(&self, _: SessionId) -> Result<()> {
            unreachable!()
        }
        fn debug_endpoint(&self, _: SessionId) -> Result<String> {
            unreachable!()
        }
    }

    #[test]
    fn the_ego_lite_check_waits_as_long_as_the_setting_says() {
        let settings = Settings::default();
        let probe = Probe {
            answers_within: Duration::from_secs(10),
            waits: Mutex::default(),
        };
        assert!(
            !ego_lite_answers(&settings, &probe).unwrap(),
            "the default 5 s"
        );
        settings.setting_set(EGO_PROBE_SECONDS, "10").unwrap();
        assert!(ego_lite_answers(&settings, &probe).unwrap());
        assert_eq!(
            *probe.waits.lock().unwrap(),
            [Duration::from_secs(5), Duration::from_secs(10)]
        );
    }

    /// A browser that never gets ready and never exits: `open` and `stop` wait it out.
    struct Slow;
    struct NeverReady;
    impl StartedSupervisor for NeverReady {
        fn exited(&mut self) -> Result<Option<String>> {
            Ok(None)
        }
        fn detach(self: Box<Self>) {}
    }
    impl SessionRuntime for Slow {
        fn has_browser(&self, _: &Path) -> bool {
            true
        }
        fn preflight(&self, _: SessionId) -> Result<()> {
            Ok(())
        }
        fn is_alive(&self, _: SessionId) -> bool {
            true
        }
        fn lock(&self, _: SessionId) -> Result<RuntimeLock> {
            Ok(Box::new(()))
        }
        fn start(
            &self,
            _: SessionId,
            _: Generation,
            _: &LaunchOptions<'_>,
        ) -> Result<Box<dyn StartedSupervisor>> {
            Ok(Box::new(NeverReady))
        }
        fn close(&self, _: SessionId) -> Result<()> {
            Ok(())
        }
        fn resolve_browser(&self, _: &BrowserChoice, _: Duration) -> Result<ResolvedBrowser> {
            unreachable!()
        }
        fn lock_reason(&self, _: SessionId) -> Result<Option<LockReason>> {
            unreachable!()
        }
        fn unlock(&self, _: SessionId) -> Result<()> {
            unreachable!()
        }
        fn remove_socket(&self, _: SessionId) {
            unreachable!()
        }
        fn remove_profile(&self, _: SessionId) -> Result<()> {
            unreachable!()
        }
        fn debug_endpoint(&self, _: SessionId) -> Result<String> {
            unreachable!()
        }
    }

    fn new_session(store: &dyn Store) -> SessionId {
        let name = keyharbor_domain::SessionName::parse("work").unwrap();
        store.create_described(name, None).unwrap().id
    }

    /// How long `open` waited before it gave up on the slow browser.
    fn open_gives_up_after(store: &dyn Store, id: SessionId) -> Duration {
        let clock = ManualClock::at_unix_seconds(1_000);
        let before = clock.now();
        let browser = ResolvedBrowser {
            kind: BrowserKind::Chrome,
            executable: "/bin/chrome".into(),
        };
        let options = OpenOptions {
            url: None,
            cdp: false,
            browser: &browser,
            headless: true,
            agent: "cli",
        };
        let error = open(store, &Slow, &clock, id, &options).unwrap_err();
        assert!(
            error.to_string().contains("did not report ready"),
            "{error}"
        );
        clock.now() - before
    }

    #[test]
    fn open_waits_as_long_as_the_start_setting_says() {
        let e = crate::tests::env();
        let store = crate::tests::open_store(&e.data);
        let id = new_session(&store);
        let waited = open_gives_up_after(&store, id);
        assert!(waited >= Duration::from_secs(15) && waited < Duration::from_secs(16));
        store.force_stopped(id).unwrap();
        store.setting_set(SESSION_START_SECONDS, "5").unwrap();
        let waited = open_gives_up_after(&store, id);
        assert!(waited >= Duration::from_secs(5) && waited < Duration::from_secs(6));
    }

    #[test]
    fn stop_waits_as_long_as_the_stop_setting_says() {
        let e = crate::tests::env();
        let store = crate::tests::open_store(&e.data);
        let id = new_session(&store);
        store.setting_set(SESSION_STOP_SECONDS, "40").unwrap();
        let generation = store
            .reserve(id, false, BrowserKind::Chrome)
            .unwrap()
            .unwrap();
        assert!(super::supervision::mark_running(&store, id, generation).unwrap());
        let clock = ManualClock::at_unix_seconds(1_000);
        let before = clock.now();
        let error = stop(&store, &Slow, &clock, id, "cli").unwrap_err();
        assert!(
            error.to_string().contains("did not exit in time"),
            "{error}"
        );
        let waited = clock.now() - before;
        assert!(waited >= Duration::from_secs(40) && waited < Duration::from_secs(41));
    }
}
