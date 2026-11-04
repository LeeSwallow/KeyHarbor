//! `vault init`, `vault rotate-root` and `vault rotate-domain`: the vault's keys. The key
//! hierarchy is core's (07 §7.5, §7.8); these print only what changed.

use crate::compose::App;
use anyhow::Result;
use keyharbor_domain::vault::Domain;

pub fn init(app: &App) -> Result<()> {
    let vault = app.init_vault()?;
    println!("vault\t{}", vault.vault_id().to_uuid_string());
    Ok(())
}

pub fn rotate_root(app: &App) -> Result<()> {
    app.vault()?.rotate_root()?;
    println!("rotated\troot");
    Ok(())
}

pub fn rotate_domain(app: &App, domain: Domain) -> Result<()> {
    app.vault()?.rotate_domain(domain)?;
    println!("rotated\t{}", domain.as_str());
    Ok(())
}
