use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    fs,
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Debug, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub state: String,
    pub generation: Option<String>,
}

pub struct HandoffSnapshot {
    pub generation: String,
    pub status: String,
    pub session_state: String,
    pub session_generation: Option<String>,
}

pub struct Store {
    pub root: PathBuf,
    conn: Connection,
}

pub fn validate_url(value: &str) -> Result<String> {
    if value.len() > 2048 {
        bail!("URL exceeds 2048 bytes");
    }
    let url = url::Url::parse(value).context("invalid URL")?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("URL must not include user information, query, or fragment");
    }
    let local = match url.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    };
    if url.host().is_none() || !(url.scheme() == "https" || (url.scheme() == "http" && local)) {
        bail!("URL must use HTTPS or HTTP on localhost");
    }
    Ok(url.into())
}

pub fn validate_origin(value: &str) -> Result<String> {
    let url = url::Url::parse(&validate_url(value)?)?;
    if url.path() != "/" {
        bail!("authentication origin must not contain a path");
    }
    Ok(url.origin().ascii_serialization())
}

impl Store {
    pub fn backup(&self, destination: &Path) -> Result<()> {
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let temporary = parent.join(format!(".keyharbor-backup-{}", Uuid::new_v4()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&temporary)
            .context("create backup staging file")?;
        let result = (|| -> Result<()> {
            let mut target = Connection::open(&temporary)?;
            {
                let backup = rusqlite::backup::Backup::new(&self.conn, &mut target)?;
                let deadline = Instant::now() + Duration::from_secs(30);
                loop {
                    if Instant::now() >= deadline {
                        bail!("backup timed out");
                    }
                    match backup.step(128)? {
                        rusqlite::backup::StepResult::Done => break,
                        rusqlite::backup::StepResult::More => (),
                        _ => std::thread::sleep(Duration::from_millis(50)),
                    }
                }
            }
            target.pragma_update(None, "journal_mode", "DELETE")?;
            let check: String = target.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
            if check != "ok" {
                bail!("backup integrity check failed");
            }
            drop(target);
            file.sync_all()?;
            fs::hard_link(&temporary, destination).context(
                "publish backup; destination must not exist and must support hard links",
            )?;
            Ok(())
        })();
        drop(file);
        let cleanup = fs::remove_file(&temporary);
        match (result, cleanup) {
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) => {
                Err(error).context("backup published but staging file could not be removed")
            }
            (Ok(()), Ok(())) => Ok(()),
        }
    }

    pub fn open(root: &Path) -> Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(root)
                .context("create data directory")?;
            let metadata = fs::symlink_metadata(root)?;
            if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
                bail!(
                    "data directory must not be accessible by other users: {}",
                    root.display()
                );
            }
        }
        #[cfg(not(unix))]
        fs::create_dir_all(root).context("create data directory")?;
        let conn = Connection::open(root.join("keyharbor.sqlite3"))?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE,
            state TEXT NOT NULL CHECK(state IN ('stopped','starting','running','awaiting_user','stopping','failed')),
            generation TEXT
        );
        CREATE TABLE IF NOT EXISTS account_refs (
            session_id TEXT PRIMARY KEY REFERENCES sessions(id),
            provider TEXT NOT NULL,
            item TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS handoffs (
            session_id TEXT PRIMARY KEY REFERENCES sessions(id),
            generation TEXT NOT NULL,
            request_id TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('pending','resumed'))
        );
        CREATE TABLE IF NOT EXISTS handoff_origins (
            session_id TEXT PRIMARY KEY REFERENCES sessions(id),
            request_id TEXT NOT NULL,
            origin TEXT
        );")?;
        Ok(Self {
            root: root.to_owned(),
            conn,
        })
    }

    pub fn create(&self, name: &str) -> Result<Session> {
        let name = name.trim();
        if name.is_empty() || name.len() > 120 || name.chars().any(char::is_control) {
            bail!("session name must be 1–120 bytes without control characters");
        }
        let id = Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT INTO sessions (id,name,state) VALUES (?1,?2,'stopped')",
            params![id, name],
        )?;
        self.get(&id)
    }

    pub fn get(&self, id: &str) -> Result<Session> {
        self.conn
            .query_row(
                "SELECT id,name,state,generation FROM sessions WHERE id=?1",
                [id],
                |row| {
                    Ok(Session {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        state: row.get(2)?,
                        generation: row.get(3)?,
                    })
                },
            )
            .optional()?
            .context("session not found")
    }

    pub fn list(&self) -> Result<Vec<Session>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id,name,state,generation FROM sessions ORDER BY name")?;
        let rows = stmt.query_map([], |row| {
            Ok(Session {
                id: row.get(0)?,
                name: row.get(1)?,
                state: row.get(2)?,
                generation: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn bind_account(&self, id: &str, item: &str) -> Result<()> {
        let item = item.trim();
        if item.is_empty() || item.len() > 120 || item.chars().any(char::is_control) {
            bail!("account item name must be 1–120 bytes without control characters");
        }
        let changed = self.conn.execute(
            "INSERT INTO account_refs(session_id,provider,item)
             SELECT id,'openkeylet',?1 FROM sessions WHERE id=?2 AND state='stopped'
             ON CONFLICT(session_id) DO UPDATE SET provider=excluded.provider,item=excluded.item",
            params![item, id],
        )?;
        if changed != 1 {
            bail!("session must exist and be stopped to change its account reference");
        }
        Ok(())
    }

    pub fn account(&self, id: &str) -> Result<Option<(String, String)>> {
        self.get(id)?;
        Ok(self
            .conn
            .query_row(
                "SELECT provider,item FROM account_refs WHERE session_id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?)
    }

    pub fn unbind_account(&self, id: &str) -> Result<()> {
        let changed = self.conn.execute(
            "DELETE FROM account_refs WHERE session_id=?1 AND EXISTS (SELECT 1 FROM sessions WHERE id=?1 AND state='stopped')", [id]
        )?;
        if changed != 1 {
            bail!("session must be stopped and have an account reference");
        }
        Ok(())
    }

    pub fn reserve(&self, id: &str) -> Result<String> {
        let generation = Uuid::new_v4().to_string();
        let changed = self.conn.execute(
            "UPDATE sessions SET state='starting',generation=?1 WHERE id=?2 AND state='stopped'",
            params![generation, id],
        )?;
        if changed != 1 {
            bail!("session is unavailable or not found; inspect its state before retrying");
        }
        Ok(generation)
    }

    pub fn transition(&self, id: &str, generation: &str, from: &str, to: &str) -> Result<bool> {
        Ok(self.conn.execute(
            "UPDATE sessions SET state=?1 WHERE id=?2 AND generation=?3 AND state=?4",
            params![to, id, generation, from],
        )? == 1)
    }

    pub fn request_handoff(
        &self,
        id: &str,
        generation: &str,
        origin: Option<&str>,
    ) -> Result<String> {
        let origin = origin.map(validate_origin).transpose()?;
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let changed = tx.execute("UPDATE sessions SET state='awaiting_user' WHERE id=?1 AND generation=?2 AND state='running'", params![id, generation])?;
        if changed != 1 {
            bail!("session state changed or is not running");
        }
        let request_id = Uuid::new_v4().to_string();
        tx.execute("INSERT INTO handoffs(session_id,generation,request_id,status) VALUES (?1,?2,?3,'pending') ON CONFLICT(session_id) DO UPDATE SET generation=excluded.generation,request_id=excluded.request_id,status='pending'", params![id,generation,request_id])?;
        tx.execute("INSERT INTO handoff_origins(session_id,request_id,origin) VALUES (?1,?2,?3) ON CONFLICT(session_id) DO UPDATE SET request_id=excluded.request_id,origin=excluded.origin", params![id,request_id,origin])?;
        tx.commit()?;
        Ok(request_id)
    }

    pub fn pending_handoff(&self, id: &str) -> Result<Option<(String, Option<String>)>> {
        Ok(self.conn.query_row(
            "SELECT h.request_id,o.origin FROM handoffs h JOIN sessions s ON s.id=h.session_id LEFT JOIN handoff_origins o ON o.session_id=h.session_id AND o.request_id=h.request_id WHERE s.id=?1 AND s.state='awaiting_user' AND h.generation=s.generation AND h.status='pending'",
            [id], |row| Ok((row.get(0)?,row.get(1)?)),
        ).optional()?)
    }

    pub fn resume_handoff(&self, id: &str, generation: &str, request_id: &str) -> Result<()> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let changed = tx.execute("UPDATE handoffs SET status='resumed' WHERE session_id=?1 AND generation=?2 AND request_id=?3 AND status='pending'", params![id,generation,request_id])?;
        if changed != 1 {
            bail!("authentication request is no longer pending; inspect the current request");
        }
        let changed = tx.execute("UPDATE sessions SET state='running' WHERE id=?1 AND generation=?2 AND state='awaiting_user'", params![id,generation])?;
        if changed != 1 {
            bail!("session state changed or is not awaiting_user");
        }
        tx.commit()?;
        Ok(())
    }

    pub fn handoff_status(&self, id: &str, request_id: &str) -> Result<Option<HandoffSnapshot>> {
        Ok(self.conn.query_row("SELECT h.generation,h.status,s.state,s.generation FROM handoffs h JOIN sessions s ON s.id=h.session_id WHERE h.session_id=?1 AND h.request_id=?2", params![id,request_id], |row| Ok(HandoffSnapshot { generation: row.get(0)?, status: row.get(1)?, session_state: row.get(2)?, session_generation: row.get(3)? })).optional()?)
    }

    pub fn profile(&self, id: &str) -> Result<PathBuf> {
        let id = Uuid::parse_str(id).context("invalid session id")?;
        Ok(self.root.join("profiles").join(id.to_string()))
    }

    pub fn prepare_profile(&self, id: &str) -> Result<PathBuf> {
        fn directory(path: &Path) -> Result<()> {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(path) {
                Ok(()) => (),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(error) => return Err(error.into()),
            }
            if !fs::symlink_metadata(path)?.is_dir() {
                bail!(
                    "profile path must be a directory and not a symbolic link: {}",
                    path.display()
                );
            }
            Ok(())
        }
        let profile = self.profile(id)?;
        directory(profile.parent().context("invalid profile path")?)?;
        directory(&profile)?;
        Ok(profile)
    }

    pub fn lock_session(&self, id: &str) -> Result<File> {
        let id = Uuid::parse_str(id).context("invalid session id")?;
        let locks = self.root.join("locks");
        fs::create_dir_all(&locks)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(locks.join(id.to_string()))?;
        file.try_lock()
            .context("session supervisor is active; recovery is not permitted")?;
        Ok(file)
    }

    pub fn lock_mcp_session(&self, id: &str) -> Result<File> {
        let id = Uuid::parse_str(id).context("invalid session id")?;
        let locks = self.root.join("mcp-sessions");
        fs::create_dir_all(&locks)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(locks.join(id.to_string()))?;
        file.try_lock()
            .context("another scoped MCP server already owns this session")?;
        Ok(file)
    }

    pub fn supervisor_active(&self, id: &str) -> Result<bool> {
        let id = Uuid::parse_str(id).context("invalid session id")?;
        let file = match OpenOptions::new()
            .read(true)
            .write(true)
            .open(self.root.join("locks").join(id.to_string()))
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        match file.try_lock() {
            Err(std::fs::TryLockError::WouldBlock) => Ok(true),
            Ok(()) => Ok(false),
            Err(err) => Err(err.into()),
        }
    }

    pub fn require_supervisor(&self, id: &str) -> Result<()> {
        if !self.supervisor_active(id)? {
            bail!("session supervisor is not running; inspect before recovery");
        }
        Ok(())
    }

    pub fn cdp_marker(&self, id: &str, generation: &str) -> Result<PathBuf> {
        let id = Uuid::parse_str(id).context("invalid session id")?;
        let generation = Uuid::parse_str(generation).context("invalid generation")?;
        Ok(self
            .root
            .join("automation")
            .join(id.to_string())
            .join(generation.to_string()))
    }

    pub fn cdp_endpoint(&self, id: &str) -> Result<String> {
        let session = self.get(id)?;
        if session.state != "running" {
            bail!("session is not running");
        }
        let generation = session.generation.context("session has no generation")?;
        if !self.cdp_marker(id, &generation)?.is_file() {
            bail!("browser control was not enabled for this session");
        }
        self.require_supervisor(id)?;
        let file = self.profile(id)?.join("DevToolsActivePort");
        let metadata =
            fs::symlink_metadata(&file).context("browser debugging endpoint is not ready")?;
        if !metadata.is_file() || metadata.len() > 256 {
            bail!("invalid browser endpoint file");
        }
        let data = fs::read_to_string(file).context("browser debugging endpoint is not ready")?;
        let mut lines = data.lines();
        let port: u16 = lines.next().context("missing debug port")?.parse()?;
        if port == 0 {
            bail!("invalid debug port");
        }
        let path = lines.next().context("missing browser endpoint")?;
        let browser_id = path
            .strip_prefix("/devtools/browser/")
            .context("invalid browser endpoint")?;
        Uuid::parse_str(browser_id).context("invalid browser endpoint")?;
        if lines.next().is_some() {
            bail!("invalid browser endpoint");
        }
        let latest = self.get(id)?;
        if latest.state != "running" || latest.generation.as_deref() != Some(&generation) {
            bail!("session changed while retrieving browser endpoint");
        }
        Ok(format!("ws://127.0.0.1:{port}{path}"))
    }

    pub fn recover(&self, id: &str) -> Result<()> {
        let _lock = self.lock_session(id)?;
        let session = self.get(id)?;
        if session.state == "stopped" {
            bail!("session is already stopped");
        }
        let generation = session.generation.context("session has no generation")?;
        if !self.transition(id, &generation, &session.state, "stopped")? {
            bail!("session state changed; inspect before retrying");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backup_captures_committed_wal_and_never_overwrites() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        let store = Store::open(&root).unwrap();
        let session = store.create("committed").unwrap();
        assert!(root.join("keyharbor.sqlite3-wal").exists());
        let writer = Connection::open(root.join("keyharbor.sqlite3")).unwrap();
        writer
            .execute_batch("BEGIN IMMEDIATE; UPDATE sessions SET name='uncommitted'")
            .unwrap();
        let destination = tmp.path().join("snapshot.sqlite3");
        store.backup(&destination).unwrap();
        let snapshot = Connection::open(&destination).unwrap();
        let name: String = snapshot
            .query_row(
                "SELECT name FROM sessions WHERE id=?1",
                [&session.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(name, "committed");
        writer.execute_batch("ROLLBACK").unwrap();
        let existing = tmp.path().join("existing");
        fs::write(&existing, b"preserve").unwrap();
        assert!(store.backup(&existing).is_err());
        assert_eq!(fs::read(existing).unwrap(), b"preserve");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(destination).unwrap().permissions().mode() & 0o077,
                0
            );
        }
    }
    #[test]
    fn reservation_is_exclusive_and_fenced() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("data");
        let a = Store::open(&root).unwrap();
        let b = Store::open(&root).unwrap();
        let session = a.create("work").unwrap();
        a.bind_account(&session.id, "Demo account").unwrap();
        assert_eq!(
            b.account(&session.id).unwrap(),
            Some(("openkeylet".into(), "Demo account".into()))
        );
        let generation = a.reserve(&session.id).unwrap();
        assert!(b.bind_account(&session.id, "Other").is_err());
        assert!(b.unbind_account(&session.id).is_err());
        assert!(b.reserve(&session.id).is_err());
        assert!(
            !b.transition(&session.id, "old", "starting", "running")
                .unwrap()
        );
        assert!(
            b.transition(&session.id, &generation, "starting", "running")
                .unwrap()
        );
        assert!(a.profile("../escape").is_err());
        let lock = a.lock_session(&session.id).unwrap();
        assert!(b.recover(&session.id).is_err());
        drop(lock);
        b.recover(&session.id).unwrap();
        assert_eq!(a.get(&session.id).unwrap().state, "stopped");
        a.unbind_account(&session.id).unwrap();
        assert_eq!(b.account(&session.id).unwrap(), None);
        let replacement = a.reserve(&session.id).unwrap();
        assert_ne!(generation, replacement);
        assert!(
            !b.transition(&session.id, &generation, "starting", "running")
                .unwrap()
        );
    }
    #[test]
    fn url_rejects_unsafe_destinations_and_credentials() {
        assert!(validate_url("https://example.com/login").is_ok());
        assert!(validate_url("http://127.0.0.1:8080/login").is_ok());
        for url in [
            "http://example.com",
            "file:///etc/passwd",
            "https://user:secret@example.com",
            "https://example.com/?token=secret",
            "https://example.com/#secret",
        ] {
            assert!(validate_url(url).is_err(), "{url}");
        }
        assert_eq!(
            validate_origin("https://EXAMPLE.com:443/").unwrap(),
            "https://example.com"
        );
        for origin in [
            "https://example.com/login",
            "https://example.com/?token=secret",
            "https://user:secret@example.com",
            "http://example.com",
        ] {
            assert!(validate_origin(origin).is_err(), "{origin}");
        }
    }

    #[test]
    fn handoff_is_bound_to_request_and_generation() {
        let dir = tempfile::tempdir().unwrap();
        let a = Store::open(&dir.path().join("data")).unwrap();
        let b = Store::open(&dir.path().join("data")).unwrap();
        let session = a.create("handoff").unwrap();
        let generation = a.reserve(&session.id).unwrap();
        assert!(
            a.transition(&session.id, &generation, "starting", "running")
                .unwrap()
        );
        assert!(
            a.request_handoff(&session.id, &generation, Some("https://example.com/login"))
                .is_err()
        );
        assert_eq!(a.get(&session.id).unwrap().state, "running");
        let first = a
            .request_handoff(&session.id, &generation, Some("https://example.com"))
            .unwrap();
        assert!(b.request_handoff(&session.id, &generation, None).is_err());
        assert!(b.resume_handoff(&session.id, "wrong", &first).is_err());
        assert_eq!(
            a.handoff_status(&session.id, &first)
                .unwrap()
                .unwrap()
                .status,
            "pending"
        );
        assert_eq!(
            a.pending_handoff(&session.id).unwrap(),
            Some((first.clone(), Some("https://example.com".into())))
        );
        b.resume_handoff(&session.id, &generation, &first).unwrap();
        assert_eq!(
            a.handoff_status(&session.id, &first)
                .unwrap()
                .unwrap()
                .status,
            "resumed"
        );
        let second = a.request_handoff(&session.id, &generation, None).unwrap();
        assert_ne!(first, second);
        assert!(b.resume_handoff(&session.id, &generation, &first).is_err());
        assert_eq!(
            a.pending_handoff(&session.id).unwrap(),
            Some((second.clone(), None))
        );
        assert_eq!(a.get(&session.id).unwrap().state, "awaiting_user");
        b.resume_handoff(&session.id, &generation, &second).unwrap();
        assert_eq!(a.pending_handoff(&session.id).unwrap(), None);
        assert!(a.handoff_status(&session.id, &first).unwrap().is_none());
    }

    #[test]
    fn concurrent_handoff_requests_produce_one_request() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("data");
        let store = Store::open(&root).unwrap();
        let session = store.create("concurrent").unwrap();
        let generation = store.reserve(&session.id).unwrap();
        assert!(
            store
                .transition(&session.id, &generation, "starting", "running")
                .unwrap()
        );
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let mut workers = Vec::new();
        for _ in 0..2 {
            let (root, id, generation, barrier) = (
                root.clone(),
                session.id.clone(),
                generation.clone(),
                barrier.clone(),
            );
            workers.push(std::thread::spawn(move || {
                let store = Store::open(&root).unwrap();
                barrier.wait();
                store.request_handoff(&id, &generation, None)
            }));
        }
        let results: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        let request = results.into_iter().find_map(Result::ok).unwrap();
        assert_eq!(
            store
                .handoff_status(&session.id, &request)
                .unwrap()
                .unwrap()
                .status,
            "pending"
        );
    }

    #[test]
    fn endpoint_requires_current_generation_running_state_and_supervisor() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("data")).unwrap();
        let session = store.create("control").unwrap();
        let generation = store.reserve(&session.id).unwrap();
        let profile = store.profile(&session.id).unwrap();
        fs::create_dir_all(&profile).unwrap();
        fs::write(
            profile.join("DevToolsActivePort"),
            "12345\n/devtools/browser/11111111-1111-4111-8111-111111111111\n",
        )
        .unwrap();
        let marker = store.cdp_marker(&session.id, &generation).unwrap();
        fs::create_dir_all(marker.parent().unwrap()).unwrap();
        fs::write(marker, "enabled").unwrap();
        assert!(
            store
                .transition(&session.id, &generation, "starting", "running")
                .unwrap()
        );
        assert!(store.cdp_endpoint(&session.id).is_err());
        let guard = store.lock_session(&session.id).unwrap();
        assert!(
            store
                .cdp_endpoint(&session.id)
                .unwrap()
                .starts_with("ws://127.0.0.1:12345/")
        );
        assert!(
            store
                .transition(&session.id, &generation, "running", "awaiting_user")
                .unwrap()
        );
        assert!(store.cdp_endpoint(&session.id).is_err());
        assert!(
            store
                .transition(&session.id, &generation, "awaiting_user", "stopped")
                .unwrap()
        );
        drop(guard);
        let next = store.reserve(&session.id).unwrap();
        assert!(
            store
                .transition(&session.id, &next, "starting", "running")
                .unwrap()
        );
        let _guard = store.lock_session(&session.id).unwrap();
        assert!(store.cdp_endpoint(&session.id).is_err());
    }
}
