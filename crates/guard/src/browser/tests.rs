use super::*;
use crate::{
    Guard,
    test_support::{env, init, login, open_vault},
};
use keyharbor_domain::ports::browser::{Choice, Element, Focus, Page};
use keyharbor_domain::vault::Origin;
use keyharbor_domain::{
    SessionId,
    audit::AuditKind,
    policy::{Effect, LOCAL_AGENT, Resource, Rule},
    ports::{PolicyStore, Store},
    session::substitution_lock::{LockReason, UNLOCKED},
};
use keyharbor_domain::{
    ports::GrantStore,
    upload::{UploadFile, UploadPolicy},
};
use keyharbor_domain::{ports::browser::LoginFields, vault::ItemName};
use keyharbor_sqlite::SqliteStore;
use serde_json::json;
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

/// A browser whose page is `https://github.com` with one focused input (node 7), a
/// password input unless a test changes `element`.
struct Fake {
    typed: Vec<String>,
    echo: String,
    element: Element,
    /// What the last redacted snapshot was asked to hide.
    hidden: BTreeSet<u64>,
    /// The focused node; inserting text moves the focus to `focus_after_insert`.
    focused: u64,
    focus_after_insert: Option<u64>,
    /// Whether navigations have settled, and the page's navigation count; clearing an
    /// element adds `navigate_on_clear` to it (page code that navigates).
    settled: bool,
    epoch: u64,
    navigate_on_clear: u64,
    /// The sign-in inputs `login_fields` reports.
    login: LoginFields,
    pressed: Vec<String>,
    /// The `<select>` options `(value, label)` and what `select_option` was asked for.
    options: Vec<(String, String)>,
    asked: Vec<Choice>,
    /// What `set_files` put into the page.
    uploaded: Vec<std::path::PathBuf>,
    /// What the last screenshot was asked to hide, and whether hiding fails.
    shot_hidden: Option<BTreeSet<u64>>,
    conceal_fails: bool,
}
impl Default for Fake {
    fn default() -> Self {
        Self {
            typed: Vec::new(),
            echo: String::new(),
            element: Element {
                local_name: "input".into(),
                input_type: Some("password".into()),
                ..Element::default()
            },
            hidden: BTreeSet::new(),
            focused: 7,
            focus_after_insert: None,
            settled: true,
            epoch: 0,
            navigate_on_clear: 0,
            login: LoginFields {
                username: Some(7),
                password: Some(7),
            },
            pressed: Vec::new(),
            options: Vec::new(),
            asked: Vec::new(),
            uploaded: Vec::new(),
            shot_hidden: None,
            conceal_fails: false,
        }
    }
}
impl BrowserPort for Fake {
    async fn navigate(&mut self, _: &str) -> Result<Value> {
        Ok(json!({"title": self.echo}))
    }
    async fn snapshot(&mut self) -> Result<Value> {
        Ok(json!({"text": self.typed.join("|")}))
    }
    async fn screenshot(&mut self) -> Result<Value> {
        Ok(json!({}))
    }
    async fn click(&mut self, _: &Target) -> Result<Value> {
        anyhow::bail!("no element {}", self.echo)
    }
    async fn fill(&mut self, _: &Target, text: &str) -> Result<FillOutcome> {
        self.typed.push(text.to_owned());
        Ok(FillOutcome::default())
    }
    async fn type_text(&mut self, text: &str) -> Result<FillOutcome> {
        self.typed.push(text.to_owned());
        Ok(FillOutcome::default())
    }
    async fn press(&mut self, key: &str) -> Result<Value> {
        self.pressed.push(key.to_owned());
        Ok(json!({}))
    }
    async fn tabs(&mut self) -> Result<Value> {
        Ok(json!([]))
    }
    async fn new_tab(&mut self, _: Option<&str>) -> Result<Value> {
        Ok(json!({}))
    }
    async fn switch_tab(&mut self, _: &str) -> Result<Value> {
        Ok(json!({}))
    }
    async fn wait(&mut self, _: &WaitFor) -> Result<Value> {
        Ok(json!({}))
    }
    fn close(&mut self) -> Result<()> {
        Ok(())
    }
}
impl PageInput for Fake {
    async fn page(&mut self) -> Result<Page> {
        Ok(Page("p".into()))
    }
    async fn focus_element(&mut self, _: &Page, _: &Target) -> Result<u64> {
        Ok(7)
    }
    async fn clear_element(&mut self, _: &Page, _: u64) -> Result<()> {
        self.epoch += self.navigate_on_clear;
        Ok(())
    }
    async fn insert_text(&mut self, _: &Page, text: &str) -> Result<()> {
        self.typed.push(text.to_owned());
        if let Some(node) = self.focus_after_insert {
            self.focused = node;
        }
        Ok(())
    }
    async fn redacted_snapshot(&mut self, hidden: &BTreeSet<u64>) -> Result<Value> {
        self.hidden = hidden.clone();
        self.snapshot().await
    }
    async fn redacted_screenshot(&mut self, hidden: &BTreeSet<u64>) -> Result<Value> {
        self.shot_hidden = Some(hidden.clone());
        if self.conceal_fails && !hidden.is_empty() {
            anyhow::bail!("filled inputs could not be hidden; no screenshot taken");
        }
        self.screenshot().await
    }
    async fn login_fields(&mut self, _: &Page) -> Result<LoginFields> {
        Ok(self.login.clone())
    }
    async fn set_files(&mut self, _: &Page, _: u64, paths: &[std::path::PathBuf]) -> Result<()> {
        self.uploaded.extend_from_slice(paths);
        Ok(())
    }
    async fn location(&mut self, _: &Page) -> Result<String> {
        Ok(format!("https://github.com/{}", self.echo))
    }
    async fn select_option(&mut self, _: &Page, _: u64, choice: &Choice) -> Result<Option<u32>> {
        self.asked.push(choice.clone());
        let found = self.options.iter().position(|(value, label)| match choice {
            Choice::Value(want) => value == want,
            Choice::Label(want) => label == want,
            Choice::Index(_) => false,
        });
        Ok(match choice {
            Choice::Index(index) => (*index as usize) < self.options.len(),
            _ => found.is_some(),
        }
        .then(|| match choice {
            Choice::Index(index) => *index,
            _ => found.unwrap() as u32,
        }))
    }
}
impl FrameInspector for Fake {
    async fn focus(&mut self, _: &Page) -> Result<Focus> {
        Ok(Focus {
            frame: "main".into(),
            node: self.focused,
        })
    }
    async fn frame_origin(&mut self, _: &Page, _: &str) -> Option<Origin> {
        Origin::parse("https://github.com").ok()
    }
    async fn element(&mut self, _: &Page, _: u64) -> Result<Element> {
        Ok(self.element.clone())
    }
    async fn settle(&mut self, _: &Page, _: Option<&str>) -> Result<bool> {
        Ok(self.settled)
    }
    async fn navigation_epoch(&mut self, _: &Page) -> u64 {
        self.epoch
    }
}

fn guard(e: &crate::test_support::Env) -> (Guard, SharedStoreProbe) {
    let store: Box<dyn Store> = Box::new(SqliteStore::open(&e.data).unwrap());
    let store = Arc::new(Mutex::new(store));
    let (data, keys) = (e.data.clone(), e.keys.clone());
    let opener: crate::VaultOpener = Box::new(move || {
        keyharbor_core::Vault::open(
            SqliteStore::open(&data)?,
            Box::new(keyharbor_keystore::FileKeyStore::new(keys.clone())),
            crate::test_support::clock(),
        )
    });
    (
        Guard::new(
            keyharbor_crypto::random::id::<SessionId>(),
            store.clone(),
            opener,
            Arc::new(ClockHandle),
        ),
        SharedStoreProbe(store),
    )
}

/// One clock for every guard in these tests, so a test can move it.
static CLOCK: std::sync::LazyLock<keyharbor_domain::ports::ManualClock> =
    std::sync::LazyLock::new(|| keyharbor_domain::ports::ManualClock::at_unix_seconds(1000));
struct ClockHandle;
impl keyharbor_domain::ports::Clock for ClockHandle {
    fn now(&self) -> std::time::Duration {
        CLOCK.now()
    }
    fn sleep(&self, duration: std::time::Duration) {
        CLOCK.sleep(duration)
    }
}
struct SharedStoreProbe(crate::SharedStore);
impl SharedStoreProbe {
    fn kinds(&self) -> Vec<(AuditKind, String)> {
        let store = self.0.lock().unwrap();
        store
            .list_audit(None)
            .unwrap()
            .into_iter()
            .filter(|r| r.event.kind != AuditKind::VaultItemChanged)
            .map(|r| (r.event.kind, r.event.result))
            .collect()
    }
}

fn block<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(future)
}

fn select_fake(options: &[(&str, &str)]) -> Fake {
    Fake {
        element: Element {
            local_name: "select".into(),
            ..Element::default()
        },
        options: options
            .iter()
            .map(|(v, l)| ((*v).to_owned(), (*l).to_owned()))
            .collect(),
        ..Fake::default()
    }
}
fn identity(military: &str) -> keyharbor_core::NewItem {
    keyharbor_core::NewItem {
        name: keyharbor_domain::vault::ItemName::parse("me").unwrap(),
        kind: keyharbor_domain::vault::ItemKind::Identity,
        fields: vec![crate::test_support::field("military_service", military)],
        origins: vec![],
        origin_binding: None,
        status: keyharbor_core::ItemStatus::Active,
        source: keyharbor_core::ItemSource::User,
        details: Default::default(),
    }
}

fn upload_fake() -> Fake {
    Fake {
        element: Element {
            local_name: "input".into(),
            input_type: Some("file".into()),
            ..Element::default()
        },
        ..Fake::default()
    }
}
fn resume() -> (UploadPolicy, Vec<UploadFile>) {
    let policy = UploadPolicy {
        dirs: vec!["/r".into()],
        max_files: 10,
    };
    let file = UploadFile {
        asked: "/r/resume.pdf".into(),
        resolved: "/r/resume.pdf".into(),
        is_file: true,
    };
    (policy, vec![file])
}

#[test]
fn upload_records_file_names_only() {
    let e = env();
    init(&e);
    let (guard, probe) = guard(&e);
    let mut fake = upload_fake();
    let (policy, files) = resume();
    let out = block(
        guard
            .browser(&mut fake, "claude")
            .upload(&Target::Ref(7), &policy, &files),
    )
    .unwrap();
    assert_eq!(out, json!({"uploaded": ["resume.pdf"]}));
    assert_eq!(
        fake.uploaded,
        vec![std::path::PathBuf::from("/r/resume.pdf")]
    );
    assert_eq!(
        probe.kinds(),
        vec![(AuditKind::FileUploaded, "resume.pdf".into())]
    );
}

#[test]
fn upload_needs_a_file_input() {
    let e = env();
    init(&e);
    let (guard, _) = guard(&e);
    let mut fake = Fake::default();
    let (policy, files) = resume();
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .upload(&Target::Ref(7), &policy, &files),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "browser_upload needs an <input type=file>");
    assert!(fake.uploaded.is_empty());
}

#[test]
fn upload_outside_grant_origin_is_refused() {
    let e = env();
    init(&e);
    let (guard, _) = guard(&e);
    let grant = keyharbor_domain::grant::Grant {
        id: keyharbor_domain::grant::GrantId::from_bytes([9; 16]),
        session: guard.session(),
        origin: Origin::parse("https://jasoseol.com").unwrap(),
        until: None,
        created_by: "cli".into(),
        reason: None,
        created_at: 0,
        revoked_at: None,
        cleared: false,
    };
    SqliteStore::open(&e.data)
        .unwrap()
        .grant_add(&grant)
        .unwrap();
    let mut fake = upload_fake();
    let (policy, files) = resume();
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .upload(&Target::Ref(7), &policy, &files),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: upload (out-of-grant)");
    assert!(fake.uploaded.is_empty());
}

#[test]
fn upload_follows_the_persons_rule() {
    let e = env();
    init(&e);
    let (guard, _) = guard(&e);
    let mut fake = upload_fake();
    let (mut policy, files) = resume();
    policy.dirs.clear();
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .upload(&Target::Ref(7), &policy, &files),
    )
    .unwrap_err();
    assert!(
        err.to_string().starts_with("upload_dirs is not set"),
        "{err}"
    );
    assert!(fake.uploaded.is_empty());
}

#[test]
fn select_by_label() {
    let e = env();
    init(&e);
    let (guard, _) = guard(&e);
    let mut fake = select_fake(&[("1", "Served"), ("2", "Exempt")]);
    let out = block(
        guard
            .browser(&mut fake, "claude")
            .select(&Target::Ref(7), &Choice::Label("Exempt".into())),
    )
    .unwrap();
    assert_eq!(out, json!({"selected": {"index": 1}}));
}

#[test]
fn select_needs_a_select_element() {
    let e = env();
    init(&e);
    let (guard, _) = guard(&e);
    let mut fake = Fake::default();
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .select(&Target::Ref(7), &Choice::Index(0)),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "browser_select needs a <select> element");
    assert!(fake.asked.is_empty());
}

#[test]
fn select_placeholder_is_resolved_and_masked() {
    let e = env();
    init(&e).add(identity("KH-SYNTH-served")).unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = select_fake(&[("a", "KH-SYNTH-exempt"), ("b", "KH-SYNTH-served")]);
    let out = block(async {
        let mut browser = guard.browser(&mut fake, "claude");
        let out = browser
            .select(
                &Target::Ref(7),
                &Choice::Label("{{secret:me.military_service}}".into()),
            )
            .await
            .unwrap();
        browser.snapshot().await.unwrap();
        out
    });
    assert_eq!(out, json!({"selected": {"index": 1}}));
    assert_eq!(fake.asked, vec![Choice::Label("KH-SYNTH-served".into())]);
    assert!(
        fake.hidden.contains(&7),
        "the chosen select is redacted after"
    );
    assert_eq!(
        probe.kinds(),
        vec![
            (AuditKind::SecretUsed, "ok".into()),
            (AuditKind::FirstUseOrigin, "ok".into())
        ]
    );
}

#[test]
fn select_refuses_a_password_field_value() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-90")).unwrap();
    let (guard, _) = guard(&e);
    let mut fake = select_fake(&[("x", "KH-SYNTH-pw-90")]);
    let err = block(guard.browser(&mut fake, "claude").select(
        &Target::Ref(7),
        &Choice::Value("{{secret:github.password}}".into()),
    ))
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: github (field-kind)");
    assert!(fake.asked.is_empty());
}

#[test]
fn unmatched_placeholder_does_not_echo_value() {
    let e = env();
    init(&e).add(identity("KH-SYNTH-other")).unwrap();
    let (guard, _) = guard(&e);
    let mut fake = select_fake(&[("1", "Served")]);
    let err = block(guard.browser(&mut fake, "claude").select(
        &Target::Ref(7),
        &Choice::Label("{{secret:me.military_service}}".into()),
    ))
    .unwrap_err()
    .to_string();
    assert_eq!(err, "no option matches");
    assert!(!fake.hidden.contains(&7));
}

#[test]
fn placeholders_are_resolved_into_the_page_and_masked_on_the_way_out() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-77")).unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = Fake::default();
    block(async {
        let mut browser = guard.browser(&mut fake, "claude");
        let target = Target::Ref(7);
        let out = browser
            .fill(&target, "{{secret:github.password}}")
            .await
            .unwrap();
        assert_eq!(out, FillOutcome::default());
        // The page has the value; the agent sees only the placeholder.
        let snap = browser.snapshot().await.unwrap();
        assert_eq!(snap, json!({"text": "{{secret:github.password}}"}));
    });
    assert_eq!(fake.typed, vec!["KH-SYNTH-pw-77".to_owned()]);
    assert_eq!(probe.kinds(), vec![(AuditKind::SecretUsed, "ok".into())]);
    // Errors are masked too.
    fake.echo = "KH-SYNTH-pw-77".into();
    let err = block(guard.browser(&mut fake, "claude").click(&Target::Ref(1)))
        .unwrap_err()
        .to_string();
    assert_eq!(err, "no element {{secret:github.password}}");
    assert!(open_vault(&e).is_ok());
}

#[test]
fn refusals_type_nothing_and_are_recorded() {
    let e = env();
    let v = init(&e);
    v.add(login("github", "KH-SYNTH-pw-78")).unwrap();
    let mut bound_elsewhere = login("bank", "KH-SYNTH-bank");
    bound_elsewhere.origins = vec![Origin::parse("https://bank.example").unwrap()];
    v.add(bound_elsewhere).unwrap();
    SqliteStore::open(&e.data)
        .unwrap()
        .policy_set(&Rule::new("codex", Resource::Item, "github", Effect::Deny).unwrap())
        .unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = Fake::default();
    let cases = [
        (
            "codex",
            "{{secret:github.password}}",
            "denied: github (policy)",
        ),
        (
            "claude",
            "{{secret:bank.password}}",
            "denied: bank (origin)",
        ),
        (
            "claude",
            "{{secret:nope.password}}",
            "denied: nope (not_found)",
        ),
        (
            "claude",
            "{{secret:github.cvc}}",
            "denied: github (not_found)",
        ),
    ];
    for (agent, text, expected) in cases {
        let err = block(guard.browser(&mut fake, agent).type_text(text)).unwrap_err();
        assert_eq!(err.to_string(), expected);
    }
    assert!(fake.typed.is_empty());
    let kinds = probe.kinds();
    assert_eq!(kinds.len(), 4);
    assert!(kinds.iter().all(|(k, _)| *k == AuditKind::SecretDenied));
    // Text without placeholders passes through untouched.
    block(guard.browser(&mut fake, "codex").type_text("hello")).unwrap();
    assert_eq!(fake.typed, vec!["hello".to_owned()]);
}

#[test]
fn password_fields_go_only_into_password_inputs() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-79")).unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = Fake {
        element: Element {
            local_name: "input".into(),
            input_type: Some("text".into()),
            ..Element::default()
        },
        ..Fake::default()
    };
    let target = Target::Ref(7);
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .fill(&target, "{{secret:github.password}}"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: github (field-kind)");
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .type_text("{{secret:github.password}}"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: github (field-kind)");
    assert!(fake.typed.is_empty());
    assert_eq!(
        probe.kinds(),
        vec![
            (AuditKind::SecretDenied, "field-kind".into()),
            (AuditKind::SecretDenied, "field-kind".into())
        ]
    );
    // The username may go into a text input; a text input that declares itself a
    // password field takes the password.
    block(
        guard
            .browser(&mut fake, "claude")
            .fill(&target, "{{secret:github.username}}"),
    )
    .unwrap();
    fake.element.autocomplete = vec!["current-password".into()];
    block(
        guard
            .browser(&mut fake, "claude")
            .fill(&target, "{{secret:github.password}}"),
    )
    .unwrap();
    assert_eq!(fake.typed, vec!["me@x.com", "KH-SYNTH-pw-79"]);
}

#[test]
fn substituted_elements_stay_redacted_in_snapshots() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-80")).unwrap();
    let (guard, _) = guard(&e);
    let mut fake = Fake::default();
    block(guard.browser(&mut fake, "claude").snapshot()).unwrap();
    assert!(fake.hidden.is_empty());
    // Plain text taints nothing; a substitution taints the element for the session,
    // whatever later happens to its value.
    block(
        guard
            .browser(&mut fake, "claude")
            .fill(&Target::Ref(7), "plain"),
    )
    .unwrap();
    block(guard.browser(&mut fake, "claude").snapshot()).unwrap();
    assert!(fake.hidden.is_empty());
    block(
        guard
            .browser(&mut fake, "claude")
            .fill(&Target::Ref(7), "{{secret:github.password}}"),
    )
    .unwrap();
    block(guard.browser(&mut fake, "codex").snapshot()).unwrap();
    assert_eq!(fake.hidden, BTreeSet::from([7]));
}

#[test]
fn screenshots_hide_the_elements_that_received_a_value() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-82")).unwrap();
    let (guard, _) = guard(&e);
    let mut fake = Fake::default();
    block(guard.browser(&mut fake, "claude").screenshot()).unwrap();
    assert_eq!(fake.shot_hidden, Some(BTreeSet::new()));
    block(
        guard
            .browser(&mut fake, "claude")
            .fill(&Target::Ref(7), "{{secret:github.password}}"),
    )
    .unwrap();
    block(guard.browser(&mut fake, "codex").screenshot()).unwrap();
    assert_eq!(fake.shot_hidden, Some(BTreeSet::from([7])));
}

#[test]
fn no_screenshot_when_filled_elements_cannot_be_hidden() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-83")).unwrap();
    let (guard, _) = guard(&e);
    let mut fake = Fake {
        conceal_fails: true,
        ..Fake::default()
    };
    block(
        guard
            .browser(&mut fake, "claude")
            .fill(&Target::Ref(7), "{{secret:github.password}}"),
    )
    .unwrap();
    let err = block(guard.browser(&mut fake, "claude").screenshot()).unwrap_err();
    assert!(err.to_string().contains("no screenshot taken"), "{err}");
}

#[test]
fn text_goes_only_into_elements_that_take_text() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-81")).unwrap();
    let (guard, probe) = guard(&e);
    for local_name in ["div", "iframe", "object", "button", "body"] {
        let mut fake = Fake {
            element: Element {
                local_name: local_name.into(),
                ..Element::default()
            },
            ..Fake::default()
        };
        let err = block(
            guard
                .browser(&mut fake, "claude")
                .type_text("{{secret:github.username}}"),
        )
        .unwrap_err();
        assert_eq!(err.to_string(), "denied: github (focus)", "{local_name}");
        assert!(fake.typed.is_empty());
    }
    assert_eq!(probe.kinds().len(), 5);
}

#[test]
fn focus_moving_during_input_locks_substitution_for_the_session() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-82")).unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = Fake {
        focus_after_insert: Some(8),
        ..Fake::default()
    };
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .type_text("{{secret:github.password}}"),
    )
    .unwrap_err();
    assert!(err.to_string().contains("substitution locked"), "{err}");
    assert!(guard.is_locked());
    // Every later substitution is refused, for every agent; plain text still works.
    fake.focus_after_insert = None;
    fake.focused = 7;
    let err = block(
        guard
            .browser(&mut fake, "codex")
            .fill(&Target::Ref(7), "{{secret:github.username}}"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: github (locked)");
    block(guard.browser(&mut fake, "codex").type_text("plain")).unwrap();
    assert_eq!(fake.typed, vec!["KH-SYNTH-pw-82", "plain"]);
    assert_eq!(
        probe.kinds(),
        vec![
            (AuditKind::SecretUsed, "ok".into()),
            (AuditKind::SubstitutionLocked, "focus_moved".into()),
            (AuditKind::SecretDenied, "locked".into()),
        ]
    );
}

#[test]
fn requests_carrying_vault_values_are_refused_and_lock_the_session() {
    let e = env();
    let v = init(&e);
    v.add(login("github", "KH-SYNTH-pw-83")).unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = Fake::default();
    // A guess list holding the value, raw or encoded, gets no answer either way.
    for url in [
        "https://x.example/?q=1111.KH-SYNTH-pw-83.2222",
        "https://x.example/#KH-SYNTH-pw-83",
    ] {
        let err = block(guard.browser(&mut fake, "claude").navigate(url)).unwrap_err();
        assert!(
            err.to_string()
                .starts_with("denied: the request contains a vault value"),
            "{err}"
        );
        assert!(!err.to_string().contains("github"));
    }
    // Harmless requests still work until the limit.
    block(
        guard
            .browser(&mut fake, "claude")
            .navigate("https://x.example/"),
    )
    .unwrap();
    assert!(!guard.is_locked());
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .type_text("KH-SYNTH-pw-83"),
    )
    .unwrap_err();
    assert!(err.to_string().contains("vault value"));
    assert!(guard.is_locked());
    // After the third probe nothing carrying text is answered, and nothing substitutes.
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .navigate("https://x.example/"),
    )
    .unwrap_err();
    assert!(err.to_string().contains("accepts no more text"), "{err}");
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .fill(&Target::Ref(7), "{{secret:github.password}}"),
    )
    .unwrap_err();
    assert!(err.to_string().contains("accepts no more text"), "{err}");
    block(guard.browser(&mut fake, "claude").snapshot()).unwrap();
    assert!(fake.typed.is_empty());
    let kinds = probe.kinds();
    assert_eq!(
        kinds
            .iter()
            .filter(|(k, _)| *k == AuditKind::MaskProbe)
            .count(),
        3
    );
    assert!(kinds.contains(&(AuditKind::SubstitutionLocked, "mask_probe".into())));
}

#[test]
fn a_person_lifting_a_probe_lock_lets_the_session_take_text_and_substitute_again() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-87")).unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = Fake::default();
    for _ in 0..crate::guard::MAX_PROBES {
        let err = block(
            guard
                .browser(&mut fake, "claude")
                .type_text("KH-SYNTH-pw-87"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("vault value"), "{err}");
    }
    assert_eq!(guard.lock_reason(), Some(LockReason::MaskProbe));
    assert_eq!(guard.unlock(LOCAL_AGENT), Some(LockReason::MaskProbe));
    assert_eq!(guard.lock_reason(), None);
    // The count starts over: plain text goes through and placeholders resolve again.
    block(
        guard
            .browser(&mut fake, "claude")
            .navigate("https://x.example/"),
    )
    .unwrap();
    block(
        guard
            .browser(&mut fake, "claude")
            .type_text("{{secret:github.password}}"),
    )
    .unwrap();
    assert_eq!(fake.typed, vec!["KH-SYNTH-pw-87"]);
    // Lifting it again changes nothing and records nothing.
    assert_eq!(guard.unlock(LOCAL_AGENT), None);
    let kinds = probe.kinds();
    let locks: Vec<&(AuditKind, String)> = kinds
        .iter()
        .filter(|(k, _)| *k == AuditKind::SubstitutionLocked)
        .collect();
    assert_eq!(
        locks,
        [
            &(AuditKind::SubstitutionLocked, "mask_probe".into()),
            &(AuditKind::SubstitutionLocked, UNLOCKED.into()),
        ]
    );
    // The record names who lifted it and holds no value.
    let store = guard.store().lock().unwrap();
    let lifted = store
        .list_audit(None)
        .unwrap()
        .into_iter()
        .find(|r| r.event.result == UNLOCKED)
        .unwrap();
    assert_eq!(lifted.event.agent, LOCAL_AGENT);
    assert_eq!(lifted.event.item_name, None);
}

#[test]
fn a_person_lifting_a_focus_lock_lets_placeholders_resolve_again() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-88")).unwrap();
    let (guard, _probe) = guard(&e);
    let mut fake = Fake {
        focus_after_insert: Some(8),
        ..Fake::default()
    };
    block(
        guard
            .browser(&mut fake, "claude")
            .type_text("{{secret:github.password}}"),
    )
    .unwrap_err();
    assert_eq!(guard.lock_reason(), Some(LockReason::FocusMoved));
    assert_eq!(guard.unlock(LOCAL_AGENT), Some(LockReason::FocusMoved));
    fake.focus_after_insert = None;
    fake.focused = 7;
    block(
        guard
            .browser(&mut fake, "claude")
            .fill(&Target::Ref(7), "{{secret:github.username}}"),
    )
    .unwrap();
}

#[test]
fn navigations_under_way_or_during_the_checks_stop_substitution() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-84")).unwrap();
    let (guard, probe) = guard(&e);
    // A click started a navigation that has not finished.
    let mut fake = Fake {
        settled: false,
        ..Fake::default()
    };
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .type_text("{{secret:github.password}}"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: github (navigating)");
    // Page code navigates between the checks and the input.
    let mut fake = Fake {
        navigate_on_clear: 1,
        ..Fake::default()
    };
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .fill(&Target::Ref(7), "{{secret:github.password}}"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: github (navigating)");
    assert!(fake.typed.is_empty());
    assert_eq!(
        probe.kinds(),
        vec![
            (AuditKind::SecretDenied, "navigating".into()),
            (AuditKind::SecretDenied, "navigating".into())
        ]
    );
}

#[test]
fn a_flood_of_refusals_is_recorded_as_one_counter_per_second() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-85")).unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = Fake::default();
    for _ in 0..25 {
        assert!(
            block(
                guard
                    .browser(&mut fake, "claude")
                    .type_text("{{secret:nope.x}}")
            )
            .is_err()
        );
    }
    CLOCK.advance(std::time::Duration::from_secs(1));
    assert!(
        block(
            guard
                .browser(&mut fake, "claude")
                .type_text("{{secret:nope.x}}")
        )
        .is_err()
    );
    let kinds = probe.kinds();
    let denied = kinds
        .iter()
        .filter(|(k, _)| *k == AuditKind::SecretDenied)
        .count();
    assert_eq!(denied, crate::guard::MAX_DENIALS_PER_SECOND as usize + 1);
    assert!(
        kinds.contains(&(AuditKind::DeniedSuppressed, "15".into())),
        "{kinds:?}"
    );
    // Whatever is left when the session ends is counted too.
    for _ in 0..12 {
        let _ = block(
            guard
                .browser(&mut fake, "claude")
                .type_text("{{secret:nope.x}}"),
        );
    }
    drop(guard);
    assert!(
        probe
            .kinds()
            .contains(&(AuditKind::DeniedSuppressed, "3".into()))
    );
}

#[test]
fn api_keys_need_an_origin_and_personal_details_note_their_first_origin() {
    use crate::test_support::field;
    use keyharbor_domain::vault::{ItemKind, ItemName};
    let e = env();
    let v = init(&e);
    let mut key = login("openai", "");
    key.kind = ItemKind::Secret;
    key.fields = vec![field("value", "KH-SYNTH-key-86")];
    key.origins = vec![];
    v.add(key).unwrap();
    let mut me = login("me", "");
    me.kind = ItemKind::Identity;
    me.fields = vec![field("email", "kh-synth-86@example.com")];
    me.origins = vec![];
    v.add(me).unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = Fake::default();
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .type_text("{{secret:openai.value}}"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: openai (no-origin)");
    // Listing an origin lets it go there, and only there.
    v.edit(
        &ItemName::parse("openai").unwrap(),
        keyharbor_core::ItemChange {
            origins: Some(vec![Origin::parse("https://platform.openai.com").unwrap()]),
            ..Default::default()
        },
    )
    .unwrap();
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .type_text("{{secret:openai.value}}"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: openai (origin)");
    v.edit(
        &ItemName::parse("openai").unwrap(),
        keyharbor_core::ItemChange {
            origins: Some(vec![Origin::parse("https://github.com").unwrap()]),
            ..Default::default()
        },
    )
    .unwrap();
    block(
        guard
            .browser(&mut fake, "claude")
            .type_text("{{secret:openai.value}}"),
    )
    .unwrap();
    // An identity goes anywhere; its first use at an origin is recorded once.
    for _ in 0..2 {
        block(
            guard
                .browser(&mut fake, "claude")
                .type_text("{{secret:me.email}}"),
        )
        .unwrap();
    }
    let kinds = probe.kinds();
    assert_eq!(
        kinds
            .iter()
            .filter(|(k, _)| *k == AuditKind::FirstUseOrigin)
            .count(),
        1,
        "{kinds:?}"
    );
    assert_eq!(
        fake.typed,
        vec![
            "KH-SYNTH-key-86",
            "kh-synth-86@example.com",
            "kh-synth-86@example.com"
        ]
    );
}

#[test]
fn expired_items_are_not_typed() {
    let e = env();
    let mut item = login("github", "KH-SYNTH-pw-87");
    item.details.expiry = keyharbor_core::ExpiryChoice::On(
        keyharbor_domain::date::Expiry::parse("2020-01-01").unwrap(),
    );
    init(&e).add(item).unwrap();
    let (guard, probe) = guard(&e);
    let mut fake = Fake::default();
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .type_text("{{secret:github.password}}"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: github (expired)");
    assert!(fake.typed.is_empty());
    assert_eq!(
        probe.kinds(),
        vec![(AuditKind::SecretDenied, "expired".into())]
    );
}

#[test]
fn login_fills_both_fields_through_the_checked_path_or_nothing() {
    let e = env();
    let v = init(&e);
    v.add(login("github", "KH-SYNTH-pw-91")).unwrap();
    let mut card = login("visa", "");
    card.kind = keyharbor_domain::vault::ItemKind::Card;
    card.fields = vec![crate::test_support::field("number", "4242424242424242")];
    v.add(card).unwrap();
    let (guard, probe) = guard(&e);
    let github = ItemName::parse("github").unwrap();
    // A page with no password input gets nothing.
    let mut fake = Fake {
        login: LoginFields {
            username: Some(7),
            password: None,
        },
        ..Fake::default()
    };
    let err = block(guard.browser(&mut fake, "claude").login(&github, true)).unwrap_err();
    assert_eq!(err.to_string(), "not found: password field");
    let mut fake = Fake {
        login: LoginFields {
            username: None,
            password: Some(7),
        },
        ..Fake::default()
    };
    let err = block(guard.browser(&mut fake, "claude").login(&github, true)).unwrap_err();
    assert_eq!(err.to_string(), "not found: username field");
    assert!(fake.typed.is_empty());
    // Only login items, and the same checks as fill.
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .login(&ItemName::parse("visa").unwrap(), true),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "visa is not a login item");
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .login(&ItemName::parse("nope").unwrap(), true),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: nope (not_found)");
    let mut fake = Fake::default();
    let out = block(guard.browser(&mut fake, "claude").login(&github, true)).unwrap();
    assert_eq!(
        out,
        crate::LoginOutcome {
            filled: vec!["username", "password"],
            submitted: true,
            url: "https://github.com/".into()
        }
    );
    assert_eq!(fake.typed, vec!["me@x.com", "KH-SYNTH-pw-91"]);
    assert_eq!(fake.pressed, vec!["Enter"]);
    let used = probe
        .kinds()
        .into_iter()
        .filter(|(k, _)| *k == AuditKind::SecretUsed)
        .count();
    assert_eq!(used, 2);
    // The result is masked like any other.
    let mut fake = Fake {
        echo: "?pw=KH-SYNTH-pw-91".into(),
        ..Fake::default()
    };
    let out = block(guard.browser(&mut fake, "claude").login(&github, false)).unwrap();
    assert_eq!(out.url, "https://github.com/?pw={{secret:github.password}}");
    assert!(fake.pressed.is_empty());
}

#[test]
fn a_masker_that_cannot_be_built_withholds_without_panicking() {
    let e = env();
    init(&e).add(login("github", "KH-SYNTH-pw-90")).unwrap();
    let (guard, _probe) = guard(&e);
    let mut fake = Fake {
        echo: "KH-SYNTH-pw-90".into(),
        ..Fake::default()
    };
    fake.typed.push("KH-SYNTH-pw-90".into());
    // A response: withheld, never sent unmasked.
    crate::pep::mask::fail::next_build();
    let err = block(guard.browser(&mut fake, "claude").snapshot())
        .unwrap_err()
        .to_string();
    assert_eq!(
        err,
        "response withheld: vault values could not be loaded for masking"
    );
    // A request with agent text: refused, since it cannot be checked for values.
    crate::pep::mask::fail::next_build();
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .navigate("https://github.com"),
    )
    .unwrap_err()
    .to_string();
    assert_eq!(
        err,
        "request withheld: vault values could not be loaded for checking"
    );
    // Nothing was poisoned: the next request builds the masker and masks.
    let snap = block(guard.browser(&mut fake, "claude").snapshot()).unwrap();
    assert_eq!(snap, json!({"text": "{{secret:github.password}}"}));
}
