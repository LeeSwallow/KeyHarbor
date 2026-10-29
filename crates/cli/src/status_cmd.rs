//! `keyharbor status`: one line of counts, for people and the plugin's session hook.

use crate::compose::App;
use anyhow::Result;
use keyharbor_core::status;

pub fn run(app: &App) -> Result<()> {
    let s = status::status(&*app.store()?, &*app.runtime()?, app.clock())?;
    println!(
        "vault {}\tsessions open {}\trequests waiting {}\tdrafts {}",
        if s.vault_ready {
            "ready"
        } else {
            "not initialized"
        },
        s.sessions_open,
        s.requests_pending,
        s.drafts
    );
    Ok(())
}
