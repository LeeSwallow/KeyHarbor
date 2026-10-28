//! The persistence bounded contexts and the order their migrations run in.
//!
//! Each context is a module with `schema.rs` (Diesel `table!`), `models.rs` (rows as stored),
//! `mappers.rs` (rows ↔ domain types), `repo.rs` (the `keyharbor_domain::ports` it implements)
//! and its own migrations under `migrations/<context>/<timestamp>_<context>_<name>/`.
//! Contexts share no models; a query that needs another context's data goes through that
//! context's `mod.rs` interface.
//!
//! Adding a context (e.g. phase 4 `site_states`): create the module and its migrations
//! directory, then add one line to [`CONTEXTS`]. Migration versions are timestamps and must
//! be unique across all contexts.

use diesel::{QueryResult, SqliteConnection};
use diesel_migrations::{EmbeddedMigrations, embed_migrations};

pub(crate) mod access;
pub(crate) mod audit;
pub(crate) mod config;
pub(crate) mod session;
pub(crate) mod vault;

/// One bounded context of the schema.
pub(crate) struct ContextSpec {
    pub name: &'static str,
    pub migrations: EmbeddedMigrations,
    /// Runs after the context's migrations, for fixes plain SQL migrations cannot express.
    pub upgrade: Option<fn(&mut SqliteConnection) -> QueryResult<()>>,
}

/// Every context, in migration order.
pub(crate) const CONTEXTS: &[ContextSpec] = &[
    ContextSpec {
        name: "vault",
        migrations: embed_migrations!("migrations/vault"),
        upgrade: None,
    },
    ContextSpec {
        name: "session",
        migrations: embed_migrations!("migrations/session"),
        upgrade: Some(session::upgrade),
    },
    ContextSpec {
        name: "access",
        migrations: embed_migrations!("migrations/access"),
        upgrade: None,
    },
    ContextSpec {
        name: "audit",
        migrations: embed_migrations!("migrations/audit"),
        upgrade: None,
    },
    ContextSpec {
        name: "config",
        migrations: embed_migrations!("migrations/config"),
        upgrade: None,
    },
];
