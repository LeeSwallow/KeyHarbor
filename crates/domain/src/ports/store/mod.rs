//! Storage ports, one per persistence context (vault, session, site, request, access, audit,
//! settings and its credential templates), and the [`Store`] that bundles them.
//! `keyharbor-sqlite` implements them. Every one fails with [`StoreError`] except
//! `SettingsStore`, which still speaks `anyhow` (J-260).

pub mod access;
pub mod audit;
mod error;
pub mod request;
pub mod session;
pub mod settings;
pub mod site;
pub mod template;
pub mod vault;

pub use access::{GrantStore, PolicyStore};
pub use audit::AuditStore;
pub use error::{Staleness, StoreError};
pub use request::VaultRequestStore;
pub use session::{SessionRow, SessionStore};
pub use settings::{AUDIT_ENABLED, SettingsStore};
pub use site::SiteStore;
pub use template::TemplateStore;
pub use vault::{
    DomainKeyRow, FieldRow, ItemRow, KeyGuard, RotationGuard, RotationLock, VaultRow, VaultStore,
};

/// Everything the local database provides, as one object.
pub trait Store:
    VaultStore
    + RotationLock
    + SessionStore
    + SiteStore
    + VaultRequestStore
    + PolicyStore
    + GrantStore
    + AuditStore
    + SettingsStore
    + TemplateStore
{
}
impl<
    T: VaultStore
        + RotationLock
        + SessionStore
        + SiteStore
        + VaultRequestStore
        + PolicyStore
        + GrantStore
        + AuditStore
        + SettingsStore
        + TemplateStore,
> Store for T
{
}

/// Opens the local database. Interfaces hand the adapter's factory to the layers that need a
/// store of their own (e.g. a session supervisor), so those layers never name the adapter.
pub trait StoreFactory: Send + Sync {
    fn open(&self) -> Result<Box<dyn Store>, StoreError>;
}
