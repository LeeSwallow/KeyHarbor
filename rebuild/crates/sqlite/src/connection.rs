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

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::{QueryableByName, RunQueryDsl, sql_query, sql_types::BigInt};

    #[derive(QueryableByName)]
    struct N {
        #[diesel(sql_type = BigInt)]
        n: i64,
    }
    fn count(conn: &mut SqliteConnection, sql: &str) -> i64 {
        sql_query(sql).get_result::<N>(conn).unwrap().n
    }

    /// R-40: rebuilding the vault tables for a fourth domain keeps every row of a database
    /// made before it, and dropping vault_items cascades nothing.
    #[test]
    fn the_developer_domain_migration_keeps_existing_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        private_dir(&root).unwrap();
        let url = root.join(DATABASE_FILE);
        let mut conn = SqliteConnection::establish(url.to_str().unwrap()).unwrap();
        conn.batch_execute(PRAGMAS).unwrap();
        // The vault context as it was before R-40: the initial schema and run_allowed.
        conn.run_next_migration(Source(&CONTEXTS[0].migrations))
            .unwrap();
        conn.run_next_migration(Source(&CONTEXTS[0].migrations))
            .unwrap();
        conn.batch_execute(
            "INSERT INTO vault (id, kek_id) VALUES (zeroblob(16), zeroblob(16));
             INSERT INTO domain_keys VALUES (1, zeroblob(16), x'01'), (2, zeroblob(16), x'02'), (3, zeroblob(16), x'03');
             INSERT INTO vault_items VALUES (x'01010101010101010101010101010101', 'api', 1, 'secret', 0, 'active', 'user', x'aa', 't', 1);
             INSERT INTO vault_fields VALUES (x'01010101010101010101010101010101', 'value', x'bb');
             INSERT INTO item_origins VALUES (x'01010101010101010101010101010101', 'https://a.example');",
        )
        .unwrap();
        drop(conn);
        let mut conn = open(&root).unwrap();
        assert_eq!(count(&mut conn, "SELECT count(*) AS n FROM domain_keys"), 3);
        assert_eq!(
            count(
                &mut conn,
                "SELECT count(*) AS n FROM vault_items WHERE run_allowed = 1 AND kind = 'secret'"
            ),
            1
        );
        assert_eq!(
            count(&mut conn, "SELECT count(*) AS n FROM vault_fields"),
            1
        );
        assert_eq!(
            count(&mut conn, "SELECT count(*) AS n FROM item_origins"),
            1
        );
        // The new domain is accepted, a fifth is not, and cascades still work.
        conn.batch_execute("INSERT INTO domain_keys VALUES (4, zeroblob(16), x'04')")
            .unwrap();
        assert!(
            conn.batch_execute("INSERT INTO domain_keys VALUES (5, zeroblob(16), x'05')")
                .is_err()
        );
        conn.batch_execute("DELETE FROM vault_items").unwrap();
        assert_eq!(
            count(&mut conn, "SELECT count(*) AS n FROM vault_fields"),
            0
        );
        assert_eq!(
            count(&mut conn, "SELECT count(*) AS n FROM item_origins"),
            0
        );
    }
}
