//! Autofill and save for the person's own browser (ADR-0017, R-46): `autofill_list` (names
//! only), `autofill_fill` (the login, after Touch ID: the one reply that carries a value) and
//! `save_login` (a draft the person confirms in the app). The checks are the guard's
//! (`keyharbor_guard::autofill`); this file only translates the wire.

use super::{
    caller,
    wire::{self, HostReplyWire, Refusal, SaveLoginWire},
};
use crate::compose::App;
use keyharbor_core::Vault;
use keyharbor_domain::vault::{ItemName, Origin};
use keyharbor_guard::{
    autofill::{self, AutofillRefusal},
    capture::CaptureOutcome,
};
use zeroize::Zeroizing;

fn origin(text: &str) -> Result<Origin, Refusal> {
    Origin::parse(text).map_err(|_| Refusal::BadRequest)
}

fn vault(app: &App) -> Result<Vault, Refusal> {
    let store = app.store().map_err(|_| Refusal::Failed)?;
    keyharbor_core::open_existing(&*store, || app.vault())
        .map_err(|_| Refusal::Failed)?
        .ok_or(Refusal::NoVault)
}

pub(super) fn list(app: &App, origin_text: &str) -> Result<HostReplyWire, Refusal> {
    let items =
        autofill::candidates(&vault(app)?, &origin(origin_text)?).map_err(|_| Refusal::Failed)?;
    Ok(HostReplyWire::Logins { items })
}

/// The guard's refusal as the code the extension shows.
fn refusal(refused: AutofillRefusal) -> Refusal {
    match refused {
        AutofillRefusal::NotCandidate => Refusal::Refused,
        AutofillRefusal::NotConfirmed | AutofillRefusal::Unavailable => Refusal::NotConfirmed,
    }
}

pub(super) fn fill(
    app: &App,
    origin_text: &str,
    item: &str,
) -> Result<Zeroizing<Vec<u8>>, Refusal> {
    let (origin, item) = (
        origin(origin_text)?,
        ItemName::parse(item).map_err(|_| Refusal::BadRequest)?,
    );
    let vault = vault(app)?;
    let store = app.store().map_err(|_| Refusal::Failed)?;
    // Only a browser gets a value back; an agent that runs the host from a shell does not.
    if !caller::is_trusted(app) {
        return Err(Refusal::NotBrowser);
    }
    let fill = autofill::fill(&*store, &vault, &*app.presence(), &origin, &item)
        .map_err(|_| Refusal::Failed)?
        .map_err(refusal)?;
    wire::filled(
        fill.username.as_ref().map(|name| name.as_str()),
        fill.password.as_str(),
    )
}

pub(super) fn save(app: &App, login: SaveLoginWire) -> Result<HostReplyWire, Refusal> {
    if login.password.is_empty() {
        return Err(Refusal::BadRequest);
    }
    let origin = origin(&login.origin)?;
    let vault = vault(app)?;
    let store = app.store().map_err(|_| Refusal::Failed)?;
    match autofill::save(&*store, &vault, &origin, login.username, login.password)
        .map_err(|_| Refusal::Failed)?
    {
        CaptureOutcome::Known(item) => Ok(HostReplyWire::Known {
            item: item.as_str().to_owned(),
        }),
        CaptureOutcome::Draft(item) => Ok(HostReplyWire::Saved {
            item: item.as_str().to_owned(),
        }),
        CaptureOutcome::Off | CaptureOutcome::NotHandedOff => Err(Refusal::Failed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_login_for_another_origin_is_refused_and_an_unconfirmed_fill_is_not_confirmed() {
        assert_eq!(refusal(AutofillRefusal::NotCandidate), Refusal::Refused);
        assert_eq!(
            refusal(AutofillRefusal::NotConfirmed),
            Refusal::NotConfirmed
        );
        assert_eq!(refusal(AutofillRefusal::Unavailable), Refusal::NotConfirmed);
    }
}
