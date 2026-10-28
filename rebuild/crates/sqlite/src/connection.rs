//! Opening the database: the private data directory, connection settings and migrations.

use crate::contexts::CONTEXTS;
use anyhow::{Context, Result, anyhow, bail};
use diesel::{Connection, SqliteConnection, connection::SimpleConnection};
use diesel::{migration::MigrationSource, sqlite::Sqlite};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness};
use std::{fs, path::Path};

pub const DATABASE_FILE: &str = "keyharbor.sqlite3";

/// Connection settings, applied to every connection before anything else.
/// WAL lets readers run beside one writer; writers wait up to 5 s for the lock.
const PRAGMAS: &str = "PRAGMA busy_timeout = 5000;
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
PRAGMA secure_delete = ON;";

fn private_dir(root: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)?;
        let metadata = fs::symlink_metadata(root)?;
        if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
            bail!("data directory must be a private directory");
        }
    }
    #[cfg(not(unix))]
    fs::create_dir_all(root)?;
    Ok(())
}

/// A context's embedded migrations, borrowed from the registry.
struct Source(&'static EmbeddedMigrations);
impl MigrationSource<Sqlite> for Source {
    fn migrations(
        &self,
    ) -> diesel::migration::Result<Vec<Box<dyn diesel::migration::Migration<Sqlite>>>> {
        MigrationSource::<Sqlite>::migrations(self.0)
    }
}

pub(crate) fn open(root: &Path) -> Result<SqliteConnection> {
    private_dir(root)?;
    let path = root.join(DATABASE_FILE);
    let url = path
        .to_str()
        .context("data directory path must be valid UTF-8")?;
    let mut conn = SqliteConnection::establish(url)?;
    conn.batch_execute(PRAGMAS)?;
    // One writer migrates at a time; the others wait, then find nothing pending.
    conn.immediate_transaction(|conn| -> Result<()> {
        for context in CONTEXTS {
            conn.run_pending_migrations(Source(&context.migrations))
                .map_err(|e| anyhow!("migrating the {} context: {e}", context.name))?;
        }
        Ok(())
    })?;
    Ok(conn)
}
