//! `--expires YYYY-MM-DD | --expires-in <N>d | --no-expiry` for `vault add` and `vault edit`
//! (R-37). Only parsing lives here; what counts as weakening is core's (`weaken`).

use anyhow::{Result, anyhow};
use clap::Args;
use keyharbor_core::{ExpiryChoice, today};
use keyharbor_domain::date::{Expiry, Ttl};

#[derive(Args)]
#[group(multiple = false)]
pub struct ExpiryArgs {
    /// Last day agents may use the item (UTC)
    #[arg(long, value_name = "YYYY-MM-DD")]
    expires: Option<String>,
    /// Expire N days from today
    #[arg(long, value_name = "Nd")]
    expires_in: Option<String>,
    /// Never expire
    #[arg(long)]
    no_expiry: bool,
}

impl ExpiryArgs {
    /// The expiry asked for: `Some(None)` for none, `None` when no flag was given.
    fn requested(&self) -> Result<Option<Option<Expiry>>> {
        if self.no_expiry {
            return Ok(Some(None));
        }
        if let Some(date) = &self.expires {
            return Ok(Some(Some(Expiry::parse(date).map_err(|e| anyhow!("{e}"))?)));
        }
        if let Some(days) = &self.expires_in {
            let ttl = Ttl::parse(days).map_err(|e| anyhow!("--expires-in: {e}"))?;
            return Ok(Some(Some(ttl.from(today()))));
        }
        Ok(None)
    }

    pub fn for_new_item(&self) -> Result<ExpiryChoice> {
        Ok(match self.requested()? {
            None => ExpiryChoice::Default,
            Some(None) => ExpiryChoice::Never,
            Some(Some(expiry)) => ExpiryChoice::On(expiry),
        })
    }

    pub fn for_change(&self) -> Result<Option<Option<Expiry>>> {
        self.requested()
    }
}
