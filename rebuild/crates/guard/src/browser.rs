//! `GuardedBrowser`: the decorator every control request goes through (ADR-0014).
//!
//! It implements `BrowserPort` over a raw browser `B`. Text for `fill` and `type_text` with
//! `{{secret:item.field}}` placeholders is resolved only after the guard's checks against the
//! frame that holds the focused element (03 §3.8, J-35); everything that comes back — values
//! and error messages alike — is masked exactly once. Error messages never include input text.

use crate::{
    Guard,
    pep::substitute::{Destination, Prepared},
};
use anyhow::{Context, Result, anyhow, bail};
use keyharbor_domain::{
    audit::{AuditEvent, AuditKind},
    ports::{
        BrowserPort, FrameInspector, PageInput,
        browser::{Element, Focus, Page, WaitFor},
    },
    vault::{Origin, parse_placeholders},
};
use serde_json::{Value, json};

const PLACEHOLDER: &str = "{{secret:";

pub struct GuardedBrowser<'a, B> {
    inner: &'a mut B,
    guard: &'a Guard,
    agent: &'a str,
}

impl<'a, B> GuardedBrowser<'a, B> {
    pub(crate) fn new(inner: &'a mut B, guard: &'a Guard, agent: &'a str) -> Self {
        Self {
            inner,
            guard,
            agent,
        }
    }
}

impl<B: BrowserPort + PageInput + FrameInspector> GuardedBrowser<'_, B> {
    /// The focused element, what it is and its frame's origin, refusing (and recording) when
    /// the focus cannot be located or, for `fill`, is not on the target element.
    async fn checked_focus(
        &mut self,
        page: &Page,
        template: &str,
        expected: Option<u64>,
    ) -> Result<(Focus, Element, Option<Origin>)> {
        let refs = parse_placeholders(template).map_err(|e| anyhow!("{e}"))?;
        let first = &refs.first().context("no placeholder")?.item;
        let focus = match self.inner.focus(page).await {
            Ok(focus) => focus,
            Err(_) => return Err(self.guard.denied(self.agent, first, None, "frame")),
        };
        let origin = self.inner.frame_origin(page, &focus.frame).await;
        if expected.is_some_and(|node| node != focus.node) {
            return Err(self.guard.denied(
                self.agent,
                first,
                origin.as_ref().map(Origin::as_str),
                "focus",
            ));
        }
        let element = match self.inner.element(page, focus.node).await {
            Ok(element) => element,
            Err(_) => {
                return Err(self.guard.denied(
                    self.agent,
                    first,
                    origin.as_ref().map(Origin::as_str),
                    "focus",
                ));
            }
        };
        Ok((focus, element, origin))
    }

    /// Types resolved text, then re-checks the frame's origin (detection only: the text is
    /// already in) and records the use.
    async fn insert_prepared(
        &mut self,
        page: &Page,
        focus: &Focus,
        origin: Option<Origin>,
        prepared: Prepared,
    ) -> Result<Value> {
        // From here on the element may hold a value: snapshots redact it for the session.
        self.guard.taint(focus.node);
        let inserted = self.inner.insert_text(page, &prepared.text).await;
        drop(prepared.text);
        inserted?;
        // The focus must still be where the text was checked to go; if page code moved it,
        // the text may have gone elsewhere, so nothing more is substituted in this session.
        let moved_focus = self.inner.focus(page).await.ok().as_ref() != Some(focus);
        let after = self.inner.frame_origin(page, &focus.frame).await;
        let changed = after != origin;
        let session = self.guard.session();
        for item in &prepared.items {
            let used = AuditEvent::new(self.agent, AuditKind::SecretUsed, "ok")
                .session(session)
                .item(item.as_str())
                .origin(origin.as_ref().map(Origin::as_str));
            self.guard.record(&used);
            if changed {
                let moved = AuditEvent::new(
                    self.agent,
                    AuditKind::OriginChangedAfterFill,
                    after.as_ref().map_or("unknown", Origin::as_str),
                )
                .session(session)
                .item(item.as_str())
                .origin(origin.as_ref().map(Origin::as_str));
                self.guard.record(&moved);
            }
        }
        if moved_focus {
            self.guard.lock(self.agent, "focus_moved");
            bail!("substitution locked for this session: the focus moved while typing");
        }
        Ok(if changed {
            json!({"warning": "origin_changed_after_fill"})
        } else {
            json!({})
        })
    }

    /// Waits for navigations to finish before checking anything, so the checks see the
    /// document the text will go into (I4). Returns the page's navigation count then.
    async fn settled(&mut self, page: &Page, template: &str) -> Result<u64> {
        if self.inner.settle(page, None).await.unwrap_or(false) {
            return Ok(self.inner.navigation_epoch(page).await);
        }
        let refs = parse_placeholders(template).map_err(|e| anyhow!("{e}"))?;
        let first = &refs.first().context("no placeholder")?.item;
        Err(self.guard.denied(self.agent, first, None, "navigating"))
    }

    /// Refuses when a navigation started (or finished) in the page since `epoch`, or is still
    /// under way in the frame: the checked document may be gone (I4).
    async fn still_there(
        &mut self,
        page: &Page,
        frame: &str,
        epoch: u64,
        prepared: &Prepared,
        origin: Option<&Origin>,
    ) -> Result<()> {
        let quiet = self.inner.settle(page, Some(frame)).await.unwrap_or(false);
        if quiet && self.inner.navigation_epoch(page).await == epoch {
            return Ok(());
        }
        Err(self.guard.denied(
            self.agent,
            &prepared.items[0],
            origin.map(Origin::as_str),
            "navigating",
        ))
    }

    async fn fill_checked(&mut self, target: &Value, template: &str) -> Result<Value> {
        if !template.contains(PLACEHOLDER) {
            return self.inner.fill(target, template).await;
        }
        let page = self.inner.page().await?;
        let epoch = self.settled(&page, template).await?;
        let node = self.inner.focus_element(&page, target).await?;
        let (focus, element, origin) = self.checked_focus(&page, template, Some(node)).await?;
        let prepared = self.guard.prepare_text(
            self.agent,
            template,
            Destination {
                frame: origin.as_ref(),
                element: Some(&element),
            },
        )?;
        self.inner.clear_element(&page, node).await?;
        // Clearing ran page code; the text must still go where it was checked.
        if self.inner.focus(&page).await.ok().as_ref() != Some(&focus) {
            let first = &prepared.items[0];
            return Err(self.guard.denied(
                self.agent,
                first,
                origin.as_ref().map(Origin::as_str),
                "focus",
            ));
        }
        self.still_there(&page, &focus.frame, epoch, &prepared, origin.as_ref())
            .await?;
        self.insert_prepared(&page, &focus, origin, prepared).await
    }

    async fn type_checked(&mut self, template: &str) -> Result<Value> {
        if !template.contains(PLACEHOLDER) {
            return self.inner.type_text(template).await;
        }
        let page = self.inner.page().await?;
        let epoch = self.settled(&page, template).await?;
        let (focus, element, origin) = self.checked_focus(&page, template, None).await?;
        let prepared = self.guard.prepare_text(
            self.agent,
            template,
            Destination {
                frame: origin.as_ref(),
                element: Some(&element),
            },
        )?;
        self.still_there(&page, &focus.frame, epoch, &prepared, origin.as_ref())
            .await?;
        self.insert_prepared(&page, &focus, origin, prepared).await
    }
}

/// The CSS selector a target carries, if any.
fn selector(target: &Value) -> &str {
    target
        .get("selector")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

impl<B: BrowserPort + PageInput + FrameInspector> GuardedBrowser<'_, B> {
    /// Refuses (and masks the refusal of) requests whose text holds a vault value (I1).
    fn literals(&self, literals: &[&str]) -> Result<()> {
        self.guard.check_literals(self.agent, literals)
    }

    /// Refuses selectors outside the structural grammar of `pep::selector` (I3).
    fn target(&self, target: &Value) -> Result<()> {
        match target.get("selector").and_then(Value::as_str) {
            Some(css) => crate::pep::selector::check(css),
            None => Ok(()),
        }
    }
}

impl<B: BrowserPort + PageInput + FrameInspector> BrowserPort for GuardedBrowser<'_, B> {
    async fn navigate(&mut self, url: &str) -> Result<Value> {
        self.literals(&[url])?;
        let outcome = self.inner.navigate(url).await;
        self.guard.redact_outcome(outcome)
    }
    async fn snapshot(&mut self) -> Result<Value> {
        let hidden = self.guard.tainted();
        let outcome = self.inner.redacted_snapshot(&hidden).await;
        self.guard.redact_outcome(outcome)
    }
    async fn screenshot(&mut self) -> Result<Value> {
        let outcome = self.inner.screenshot().await;
        self.guard.redact_outcome(outcome)
    }
    async fn click(&mut self, target: &Value) -> Result<Value> {
        self.target(target)?;
        self.literals(&[selector(target)])?;
        let outcome = self.inner.click(target).await;
        self.guard.redact_outcome(outcome)
    }
    async fn fill(&mut self, target: &Value, text: &str) -> Result<Value> {
        self.target(target)?;
        self.literals(&[selector(target), text])?;
        let outcome = self.fill_checked(target, text).await;
        self.guard.redact_outcome(outcome)
    }
    async fn type_text(&mut self, text: &str) -> Result<Value> {
        self.literals(&[text])?;
        let outcome = self.type_checked(text).await;
        self.guard.redact_outcome(outcome)
    }
    async fn press(&mut self, key: &str) -> Result<Value> {
        self.literals(&[key])?;
        let outcome = self.inner.press(key).await;
        self.guard.redact_outcome(outcome)
    }
    async fn tabs(&mut self) -> Result<Value> {
        let outcome = self.inner.tabs().await;
        self.guard.redact_outcome(outcome)
    }
    async fn new_tab(&mut self, url: Option<&str>) -> Result<Value> {
        self.literals(&[url.unwrap_or_default()])?;
        let outcome = self.inner.new_tab(url).await;
        self.guard.redact_outcome(outcome)
    }
    async fn switch_tab(&mut self, id: &str) -> Result<Value> {
        self.literals(&[id])?;
        let outcome = self.inner.switch_tab(id).await;
        self.guard.redact_outcome(outcome)
    }
    async fn wait(&mut self, until: &WaitFor) -> Result<Value> {
        if let WaitFor::Selector(selector) = until {
            crate::pep::selector::check(selector)?;
            self.literals(&[selector])?;
        }
        let outcome = self.inner.wait(until).await;
        self.guard.redact_outcome(outcome)
    }
    fn close(&mut self) -> Result<Value> {
        let outcome = self.inner.close();
        self.guard.redact_outcome(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Guard,
        test_support::{env, init, login, open_vault},
    };
    use keyharbor_domain::{
        SessionId,
        audit::AuditKind,
        policy::{Effect, Resource, Rule},
        ports::{PolicyStore, Store},
    };
    use keyharbor_sqlite::SqliteStore;
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
        async fn click(&mut self, _: &Value) -> Result<Value> {
            anyhow::bail!("no element {}", self.echo)
        }
        async fn fill(&mut self, _: &Value, text: &str) -> Result<Value> {
            self.typed.push(text.to_owned());
            Ok(json!({}))
        }
        async fn type_text(&mut self, text: &str) -> Result<Value> {
            self.typed.push(text.to_owned());
            Ok(json!({}))
        }
        async fn press(&mut self, _: &str) -> Result<Value> {
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
        fn close(&mut self) -> Result<Value> {
            Ok(json!({}))
        }
    }
    impl PageInput for Fake {
        async fn page(&mut self) -> Result<Page> {
            Ok(Page("p".into()))
        }
        async fn focus_element(&mut self, _: &Page, _: &Value) -> Result<u64> {
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
                Box::new(keyharbor_vault::keystore::FileKeyStore::new(keys.clone())),
            )
        });
        (
            Guard::new(SessionId::new(), store.clone(), opener),
            SharedStoreProbe(store),
        )
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

    #[test]
    fn placeholders_are_resolved_into_the_page_and_masked_on_the_way_out() {
        let e = env();
        init(&e).add(login("github", "KH-SYNTH-pw-77")).unwrap();
        let (guard, probe) = guard(&e);
        let mut fake = Fake::default();
        block(async {
            let mut browser = guard.browser(&mut fake, "claude");
            let target = json!({"ref": 7});
            let out = browser
                .fill(&target, "{{secret:github.password}}")
                .await
                .unwrap();
            assert_eq!(out, json!({}));
            // The page has the value; the agent sees only the placeholder.
            let snap = browser.snapshot().await.unwrap();
            assert_eq!(snap, json!({"text": "{{secret:github.password}}"}));
        });
        assert_eq!(fake.typed, vec!["KH-SYNTH-pw-77".to_owned()]);
        assert_eq!(probe.kinds(), vec![(AuditKind::SecretUsed, "ok".into())]);
        // Errors are masked too.
        fake.echo = "KH-SYNTH-pw-77".into();
        let err = block(guard.browser(&mut fake, "claude").click(&json!({"ref": 1})))
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
        let target = json!({"ref": 7});
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
                .fill(&json!({"ref": 7}), "plain"),
        )
        .unwrap();
        block(guard.browser(&mut fake, "claude").snapshot()).unwrap();
        assert!(fake.hidden.is_empty());
        block(
            guard
                .browser(&mut fake, "claude")
                .fill(&json!({"ref": 7}), "{{secret:github.password}}"),
        )
        .unwrap();
        block(guard.browser(&mut fake, "codex").snapshot()).unwrap();
        assert_eq!(fake.hidden, BTreeSet::from([7]));
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
                .fill(&json!({"ref": 7}), "{{secret:github.username}}"),
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
                .fill(&json!({"ref": 7}), "{{secret:github.password}}"),
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
                .fill(&json!({"ref": 7}), "{{secret:github.password}}"),
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

    static NOW: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1000);
    fn now() -> u64 {
        NOW.load(std::sync::atomic::Ordering::SeqCst)
    }

    #[test]
    fn a_flood_of_refusals_is_recorded_as_one_counter_per_second() {
        let e = env();
        init(&e).add(login("github", "KH-SYNTH-pw-85")).unwrap();
        let (guard, probe) = guard(&e);
        let guard = guard.with_clock(now);
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
        NOW.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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
}
