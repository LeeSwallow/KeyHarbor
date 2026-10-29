//! Requests for information only the user has (08 §8.2 ③).

use crate::{
    RequestId,
    request::{VaultRequest, VaultRequestStatus},
    vault::ItemName,
};
use anyhow::Result;

pub trait VaultRequestStore {
    fn vault_request_add(&self, request: &VaultRequest) -> Result<()>;
    fn vault_request_get(&self, id: RequestId) -> Result<Option<VaultRequest>>;
    /// Requests still marked pending, oldest first (some may be past their TTL).
    fn vault_requests_pending(&self) -> Result<Vec<VaultRequest>>;
    /// Moves a pending request to `status` (with the answering item); false if it was not
    /// pending, so two answers cannot both win.
    fn vault_request_finish(
        &self,
        id: RequestId,
        status: VaultRequestStatus,
        item: Option<&ItemName>,
    ) -> Result<bool>;
}
