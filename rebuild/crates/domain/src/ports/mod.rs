//! Ports: what the use cases and the guard need from the outside world (hexagonal
//! architecture, ADR-0014). Adapters implement them — `keyharbor-sqlite` the stores,
//! `keyharbor-vault` the key store, `keyharbor-broker` the browser — and the interfaces
//! (CLI, MCP) wire concrete adapters together.

pub mod access;
pub mod audit;
pub mod keys;
pub mod session;
pub mod vault;

pub use access::PolicyStore;
pub use audit::{AUDIT_ENABLED, AuditStore, SettingsStore};
pub use keys::KeyStore;
pub use session::{SessionRow, SessionStore};
pub use vault::{DomainKeyRow, FieldRow, ItemRow, KeyGuard, VaultRow, VaultStore};

/// Everything the local database provides, as one object.
pub trait Store: VaultStore + SessionStore + PolicyStore + AuditStore + SettingsStore {}
impl<T: VaultStore + SessionStore + PolicyStore + AuditStore + SettingsStore> Store for T {}
