//! Autofill and save for the person's own browser (ADR-0017, R-46): `autofill_list` (names
//! only), `autofill_fill` (the login, after Touch ID: the one reply that carries a value) and
//! `save_login` (a draft the person confirms in the app). The checks are the guard's
//! (`keyharbor_guard::autofill`); this file only translates the wire.

use super::Refusal;
use crate::compose::App;
use keyharbor_core::Vault;
use keyharbor_domain::vault::{ItemName, Origin};
use keyharbor_guard::{autofill, capture::CaptureOutcome};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zeroize::Zeroizing;

#[derive(Deserialize)]
struct ListWire {
    origin: String,
}

#[derive(Deserialize)]
struct FillWire {
    origin: String,
    item: String,
}

/// What the page held when the person pressed save. No `Debug`: it holds values.
#[derive(Deserialize)]
struct SaveWire {
    origin: String,
    #[serde(default, deserialize_with = "maybe_secret")]
    username: Option<Zeroizing<String>>,
    #[serde(deserialize_with = "secret")]
    password: Zeroizing<String>,
}

/// The `filled` reply, borrowing the login so the only copy written is the wiped buffer
/// ([`super::seal`], #157). Fields in key order, the order a `Value` writes them in.
#[derive(Serialize)]
struct FilledWire<'a> {
    password: &'a str,
    r#type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<&'a str>,
    v: u64,
}

fn filled_reply(username: Option<&str>, password: &str) -> Result<Zeroizing<Vec<u8>>, Refusal> {
    super::seal(&FilledWire {
        password,
        r#type: "filled",
        username,
        v: super::VERSION,
    })
}

/// The string read moves into `Zeroizing` without a copy, so it is wiped when dropped.
fn secret<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Zeroizing<String>, D::Error> {
    String::deserialize(d).map(Zeroizing::new)
}
fn maybe_secret<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<Zeroizing<String>>, D::Error> {
    Ok(Option::<String>::deserialize(d)?.map(Zeroizing::new))
}

fn parse<'a, T: Deserialize<'a>>(body: &'a [u8]) -> Result<T, Refusal> {
    serde_json::from_slice(body).map_err(|_| Refusal::BadRequest)
}

fn origin(text: &str) -> Result<Origin, Refusal> {
    Origin::parse(text).map_err(|_| Refusal::BadRequest)
}

fn vault(app: &App) -> Result<Vault, Refusal> {
    let store = app.store().map_err(|_| Refusal::Failed)?;
    if store.vault_get().map_err(|_| Refusal::Failed)?.is_none() {
        return Err(Refusal::NoVault);
    }
    app.vault().map_err(|_| Refusal::Failed)
}

pub(super) fn list(app: &App, body: &[u8]) -> Result<Value, Refusal> {
    let wire: ListWire = parse(body)?;
    let names =
        autofill::candidates(&vault(app)?, &origin(&wire.origin)?).map_err(|_| Refusal::Failed)?;
    Ok(json!({"type": "logins", "items": names}))
}

pub(super) fn fill(app: &App, body: &[u8]) -> Result<Zeroizing<Vec<u8>>, Refusal> {
    let wire: FillWire = parse(body)?;
    let (origin, item) = (
        origin(&wire.origin)?,
        ItemName::parse(&wire.item).map_err(|_| Refusal::BadRequest)?,
    );
    let vault = vault(app)?;
    let store = app.store().map_err(|_| Refusal::Failed)?;
    // Only a browser gets a value back; an agent that runs the host from a shell does not.
    if !app.trusts_any_caller()
        && !super::caller::parent_path().is_some_and(|p| super::caller::is_browser(&p))
    {
        return Err(Refusal::NotBrowser);
    }
    // A login that is not for this origin is refused before Touch ID is asked.
    if !autofill::candidates(&vault, &origin)
        .map_err(|_| Refusal::Failed)?
        .iter()
        .any(|n| n == item.as_str())
    {
        return Err(Refusal::Refused);
    }
    let fill = autofill::fill(&*store, &vault, &*app.presence(), &origin, &item)
        .map_err(|_| Refusal::NotConfirmed)?;
    filled_reply(
        fill.username.as_ref().map(|name| name.as_str()),
        fill.password.as_str(),
    )
}

pub(super) fn save(app: &App, body: &[u8]) -> Result<Value, Refusal> {
    let wire: SaveWire = parse(body)?;
    if wire.password.is_empty() {
        return Err(Refusal::BadRequest);
    }
    let origin = origin(&wire.origin)?;
    let vault = vault(app)?;
    let store = app.store().map_err(|_| Refusal::Failed)?;
    match autofill::save(&*store, &vault, &origin, wire.username, wire.password)
        .map_err(|_| Refusal::Failed)?
    {
        CaptureOutcome::Known(item) => Ok(json!({"type": "known", "item": item.as_str()})),
        CaptureOutcome::Draft(item) => Ok(json!({"type": "saved", "item": item.as_str()})),
        CaptureOutcome::Off | CaptureOutcome::NotHandedOff => Err(Refusal::Failed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser_cmd::host::VERSION;

    /// The reply as it was built before #157: a `Value` with `v` added by `serve`.
    fn as_value(username: Option<&str>, password: &str) -> Vec<u8> {
        let mut reply = json!({"type": "filled", "password": password});
        if let Some(username) = username {
            reply["username"] = json!(username);
        }
        reply["v"] = json!(VERSION);
        reply.to_string().into_bytes()
    }

    #[test]
    fn filled_reply_keeps_its_bytes_in_a_wiped_buffer() {
        for (username, password) in [(Some("me@example.com"), "p\"w\\d"), (None, "pw")] {
            let body: Zeroizing<Vec<u8>> = filled_reply(username, password).unwrap();
            assert_eq!(body.as_slice(), as_value(username, password).as_slice());
            // Measured before writing: the buffer never grew, so no unwiped copy was left.
            assert_eq!(body.capacity(), body.len());
        }
    }
}
