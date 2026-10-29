//! Ports: what the use cases and the guard need from the outside world (hexagonal
//! architecture, ADR-0014). Adapters implement them — `keyharbor-sqlite` the stores and the
//! rotation lock, `keyharbor-keystore` the key store, `keyharbor-broker` the browser,
//! `keyharbor-supervisor` the session runtime — and each binary's composition root wires
//! concrete adapters together (also the [`Clock`]).

pub mod access;
pub mod audit;
pub mod browser;
pub mod clock;
pub mod confirm;
pub mod keys;
pub mod request;
pub mod runtime;
pub mod session;
pub mod site;
pub mod vault;

pub use access::PolicyStore;
pub use audit::{AUDIT_ENABLED, AuditStore, SettingsStore};
pub use browser::{BrowserPort, FrameInspector, PageInput};
pub use clock::{Clock, ManualClock};
pub use confirm::{Answer, Confirm};
pub use keys::KeyStore;
pub use request::VaultRequestStore;
pub use runtime::SessionRuntime;
pub use session::{SessionRow, SessionStore};
pub use site::SiteStore;
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
    + AuditStore
    + SettingsStore
{
}
impl<
    T: VaultStore
        + RotationLock
        + SessionStore
        + SiteStore
        + VaultRequestStore
        + PolicyStore
        + AuditStore
        + SettingsStore,
> Store for T
{
}

/// Opens the local database. Interfaces hand the adapter's factory to the layers that need a
/// store of their own (e.g. a session supervisor), so those layers never name the adapter.
pub trait StoreFactory: Send + Sync {
    fn open(&self) -> anyhow::Result<Box<dyn Store>>;
}
