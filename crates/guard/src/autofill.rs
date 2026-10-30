//! Filling a login into a page of the person's own browser (ADR-0017, R-46). This is a
//! plaintext path that ends outside KeyHarbor, so it is narrow: only an active, verified,
//! unexpired login bound to exactly that origin, only after the person confirms (Touch ID,
//! through the `Confirm` port) for this one fill, and every fill is recorded. Agents never
//! reach it: only the native host (`keyharbor browser host`) calls it, for the extension.

use crate::{
    audit,
    capture::{CaptureOutcome, store_login},
};
use anyhow::{Result, bail};
use keyharbor_core::{ItemStatus, PlaintextAccess, Vault};
use keyharbor_domain::{
    audit::{AuditEvent, AuditKind},
    ports::{Answer, Confirm, Store, browser::CapturedLogin},
    vault::{FieldName, ItemKind, ItemName, Origin, PlaceholderRef},
};
use zeroize::Zeroizing;

/// Who the usage record names for a fill: the person at their browser.
pub const FILLER: &str = "user";

/// What goes into the page. No `Debug`: it holds values.
pub struct Fill {
    pub username: Option<Zeroizing<String>>,
    pub password: Zeroizing<String>,
}

/// Stores a sign-in the person asked to save from their own browser ([이 로그인 저장]):
/// unchanged ones are left alone, anything new is a draft the person confirms in the app. No
/// Touch ID: nothing leaves the vault. Unlike capture during a handoff, the person pressing
/// save is the consent, so `capture_enabled` does not apply. Records `credential_captured`.
pub fn save(
    store: &dyn Store,
    vault: &Vault,
    origin: &Origin,
    username: Option<Zeroizing<String>>,
    password: Zeroizing<String>,
) -> Result<CaptureOutcome> {
    let login = CapturedLogin {
        origin: origin.as_str().to_owned(),
        username,
        password,
    };
    let outcome = store_login(vault, origin, login)?;
    let (result, item) = match &outcome {
        CaptureOutcome::Known(item) => ("known", item),
        CaptureOutcome::Draft(item) => ("saved", item),
        _ => return Ok(outcome),
    };
    audit::record(
        store,
        &AuditEvent::new(FILLER, AuditKind::CredentialCaptured, result)
            .item(item.as_str())
            .origin(Some(origin.as_str())),
    );
    Ok(outcome)
}

/// The logins that may fill `origin`: their names only (no value, no confirmation needed).
pub fn candidates(vault: &Vault, origin: &Origin) -> Result<Vec<String>> {
    Ok(vault
        .list()?
        .into_iter()
        .filter(|i| {
            i.kind == ItemKind::Login
                && i.status == ItemStatus::Active
                && !i.tampered
                && !i.expired
                && i.origins.iter().any(|o| o == origin.as_str())
        })
        .map(|i| i.name)
        .collect())
}

/// The login `item` for `origin`, after the person confirms this fill; records
/// `value_autofilled`. Refuses, without asking, a login that is not a candidate for `origin`.
pub fn fill(
    store: &dyn Store,
    vault: &Vault,
    confirm: &dyn Confirm,
    origin: &Origin,
    item: &ItemName,
) -> Result<Fill> {
    if !candidates(vault, origin)?
        .iter()
        .any(|n| n == item.as_str())
    {
        bail!("{} is not a login for {}", item.as_str(), origin.as_str());
    }
    let action = format!("fill the {} login into {}", item.as_str(), origin.as_str());
    match confirm.confirm(&action) {
        Answer::Yes => {}
        Answer::No => bail!("the fill was not confirmed"),
        Answer::Unavailable => bail!("no way to confirm the fill (Touch ID or the login password)"),
    }
    let access = PlaintextAccess::issue();
    let read = |field: &str| -> Result<Zeroizing<String>> {
        let reference = PlaceholderRef {
            item: item.clone(),
            field: FieldName::parse(field).map_err(|e| anyhow::anyhow!("{e}"))?,
        };
        vault.resolve(&access, &reference)
    };
    let password = read("password")?;
    let username = read("username").ok();
    audit::record(
        store,
        &AuditEvent::new(FILLER, AuditKind::ValueAutofilled, "filled")
            .item(item.as_str())
            .origin(Some(origin.as_str())),
    );
    Ok(Fill { username, password })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{env, init, login};
    use keyharbor_core::NewItem;
    use keyharbor_domain::ports::AuditStore;
    use keyharbor_sqlite::SqliteStore;
    use std::cell::Cell;

    /// Answers with `answer` and counts the questions.
    struct Person {
        answer: Answer,
        asked: Cell<u32>,
    }
    impl Confirm for Person {
        fn confirm(&self, _action: &str) -> Answer {
            self.asked.set(self.asked.get() + 1);
            self.answer
        }
    }
    fn person(answer: Answer) -> Person {
        Person {
            answer,
            asked: Cell::new(0),
        }
    }
    fn github() -> Origin {
        Origin::parse("https://github.com").unwrap()
    }
    fn name(n: &str) -> ItemName {
        ItemName::parse(n).unwrap()
    }

    #[test]
    fn a_saved_login_is_a_draft_until_confirmed_and_a_known_one_is_left_alone() {
        let e = env();
        let vault = init(&e);
        vault.add(login("github", "pw-old")).unwrap();
        let store = SqliteStore::open(&e.data).unwrap();
        let typed = |user: &str, pw: &str| {
            (
                Some(Zeroizing::new(user.to_owned())),
                Zeroizing::new(pw.to_owned()),
            )
        };
        let (user, pw) = typed("me@x.com", "pw-old");
        assert_eq!(
            save(&store, &vault, &github(), user, pw).unwrap(),
            CaptureOutcome::Known(name("github"))
        );
        let (user, pw) = typed("me@x.com", "pw-new-77");
        let CaptureOutcome::Draft(draft) = save(&store, &vault, &github(), user, pw).unwrap()
        else {
            panic!("a changed password is a draft")
        };
        assert!(draft.is_draft() && draft.base() == name("github"));
        let shop = Origin::parse("https://shop.example").unwrap();
        let (user, pw) = typed("amy", "pw-shop");
        assert!(matches!(
            save(&store, &vault, &shop, user, pw).unwrap(),
            CaptureOutcome::Draft(_)
        ));
        assert_eq!(
            candidates(&vault, &shop).unwrap(),
            Vec::<String>::new(),
            "a draft does not fill until confirmed"
        );
        let events = store.list_audit(None).unwrap();
        assert!(events.iter().any(|r| r.event.kind == AuditKind::CredentialCaptured && r.event.result == "saved"));
        assert!(!format!("{events:?}").contains("pw-new-77"));
    }

    #[test]
    fn a_confirmed_fill_gives_the_login_and_is_recorded_without_it() {
        let e = env();
        let vault = init(&e);
        vault.add(login("github", "pw-1234")).unwrap();
        let store = SqliteStore::open(&e.data).unwrap();
        let yes = person(Answer::Yes);
        let fill = fill(&store, &vault, &yes, &github(), &name("github")).unwrap();
        assert_eq!(
            (
                fill.username.as_deref().map(|u| u.as_str()),
                fill.password.as_str()
            ),
            (Some("me@x.com"), "pw-1234")
        );
        assert_eq!(yes.asked.get(), 1);
        let events = store.list_audit(None).unwrap();
        let last = &events.last().unwrap().event;
        assert_eq!(
            (last.kind, last.agent.as_str(), last.origin.as_deref()),
            (
                AuditKind::ValueAutofilled,
                "user",
                Some("https://github.com")
            )
        );
        assert!(!format!("{events:?}").contains("pw-1234"));
    }

    #[test]
    fn nothing_is_filled_without_confirmation() {
        let e = env();
        let vault = init(&e);
        vault.add(login("github", "pw")).unwrap();
        let store = SqliteStore::open(&e.data).unwrap();
        for answer in [Answer::No, Answer::Unavailable] {
            assert!(fill(&store, &vault, &person(answer), &github(), &name("github")).is_err());
        }
        assert!(
            store
                .list_audit(None)
                .unwrap()
                .iter()
                .all(|r| r.event.kind != AuditKind::ValueAutofilled)
        );
    }

    #[test]
    fn only_an_active_verified_login_bound_to_that_origin_fills_and_others_are_not_even_asked() {
        let e = env();
        let vault = init(&e);
        vault.add(login("github", "pw")).unwrap();
        vault
            .add(NewItem {
                status: ItemStatus::Draft,
                ..login("github~draft-1", "d")
            })
            .unwrap();
        let store = SqliteStore::open(&e.data).unwrap();
        let yes = person(Answer::Yes);
        let evil = Origin::parse("https://github.com.evil.example").unwrap();
        assert_eq!(candidates(&vault, &github()).unwrap(), ["github"]);
        assert!(candidates(&vault, &evil).unwrap().is_empty());
        assert!(fill(&store, &vault, &yes, &evil, &name("github")).is_err());
        assert!(fill(&store, &vault, &yes, &github(), &name("github~draft-1")).is_err());
        assert!(fill(&store, &vault, &yes, &github(), &name("nope")).is_err());
        assert_eq!(yes.asked.get(), 0, "refused before asking");
        store.execute_for_tests("UPDATE vault_items SET origin_binding = 0");
        assert!(
            candidates(&vault, &github()).unwrap().is_empty(),
            "a tampered login is no candidate"
        );
    }
}
