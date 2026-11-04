//! Ports: what the use cases and the guard need from the outside world (hexagonal
//! architecture, ADR-0014). Adapters implement them — `keyharbor-sqlite` the stores and the
//! rotation lock, `keyharbor-keystore` the key store, `keyharbor-broker` the browser,
//! `keyharbor-supervisor` the session runtime — and each binary's composition root wires
//! concrete adapters together (also the [`Clock`]).

pub mod browser;
pub mod clock;
pub mod confirm;
pub mod keys;
pub mod runtime;
pub mod store;

// The storage modules keep their old paths (`ports::vault`, `ports::session`, …).
pub use store::{access, audit, request, session, settings, site, template, vault};

pub use browser::{BrowserPort, FrameInspector, PageInput};
pub use clock::{Clock, ManualClock};
pub use confirm::{Answer, Confirm};
pub use keys::KeyStore;
pub use runtime::SessionRuntime;
pub use store::{
    AUDIT_ENABLED, AuditStore, DomainKeyRow, FieldRow, GrantStore, ItemRow, KeyGuard, PolicyStore,
    RotationGuard, RotationLock, SessionRow, SessionStore, SettingsStore, SiteStore, Staleness,
    Store, StoreError, StoreFactory, TemplateStore, VaultRequestStore, VaultRow, VaultStore,
};
