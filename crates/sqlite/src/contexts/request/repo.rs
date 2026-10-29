//! `VaultRequestStore` for the request context.

use super::{mappers, models::RequestModel, schema::vault_requests};
use crate::SqliteStore;
use anyhow::Result;
use diesel::prelude::*;
use keyharbor_domain::{
    RequestId,
    ports::VaultRequestStore,
    request::{VaultRequest, VaultRequestStatus},
    vault::ItemName,
};

const PENDING: &str = "pending";

impl VaultRequestStore for SqliteStore {
    fn vault_request_add(&self, request: &VaultRequest) -> Result<()> {
        let model = mappers::model(request)?;
        self.read(|conn| {
            diesel::insert_into(vault_requests::table)
                .values(&model)
                .execute(conn)?;
            Ok(())
        })
    }

    fn vault_request_get(&self, id: RequestId) -> Result<Option<VaultRequest>> {
        self.read(|conn| {
            vault_requests::table
                .find(id.to_string())
                .select(RequestModel::as_select())
                .first(conn)
                .optional()?
                .map(mappers::request)
                .transpose()
        })
    }

    fn vault_requests_pending(&self) -> Result<Vec<VaultRequest>> {
        self.read(|conn| {
            vault_requests::table
                .filter(vault_requests::status.eq(PENDING))
                .order((vault_requests::created_at, vault_requests::id))
                .select(RequestModel::as_select())
                .load(conn)?
                .into_iter()
                .map(mappers::request)
                .collect()
        })
    }

    fn vault_request_finish(
        &self,
        id: RequestId,
        status: VaultRequestStatus,
        item: Option<&ItemName>,
    ) -> Result<bool> {
        self.read(|conn| {
            let changed = diesel::update(
                vault_requests::table
                    .find(id.to_string())
                    .filter(vault_requests::status.eq(PENDING)),
            )
            .set((
                vault_requests::status.eq(status.as_str()),
                vault_requests::item_name.eq(item.map(ItemName::as_str)),
            ))
            .execute(conn)?;
            Ok(changed == 1)
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::SqliteStore;
    use keyharbor_domain::{
        RequestId,
        ports::VaultRequestStore,
        request::{VaultRequest, VaultRequestStatus},
        vault::{Description, ItemKind, ItemName, Origin},
    };

    fn request(byte: u8, created_at: u64) -> VaultRequest {
        VaultRequest {
            id: RequestId::from_bytes([byte; 16]),
            agent: "codex".into(),
            kind: ItemKind::Card,
            name: ItemName::parse("my-visa").unwrap(),
            reason: Description::parse("checkout at the shop").unwrap(),
            origin: Some(Origin::parse("https://shop.example").unwrap()),
            status: VaultRequestStatus::Pending,
            created_at,
            item: None,
        }
    }

    #[test]
    fn a_request_round_trips_and_is_answered_once() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(&tmp.path().join("data")).unwrap();
        let (first, second) = (request(1, 20), request(2, 10));
        store.vault_request_add(&first).unwrap();
        store.vault_request_add(&second).unwrap();
        assert_eq!(
            store.vault_request_get(first.id).unwrap(),
            Some(first.clone())
        );
        assert_eq!(
            store.vault_requests_pending().unwrap(),
            [second.clone(), first.clone()]
        );

        let item = ItemName::parse("visa").unwrap();
        assert!(
            store
                .vault_request_finish(first.id, VaultRequestStatus::Fulfilled, Some(&item))
                .unwrap()
        );
        assert!(
            !store
                .vault_request_finish(first.id, VaultRequestStatus::Declined, None)
                .unwrap()
        );
        let answered = store.vault_request_get(first.id).unwrap().unwrap();
        assert_eq!(
            (answered.status, answered.item),
            (VaultRequestStatus::Fulfilled, Some(item))
        );
        assert_eq!(store.vault_requests_pending().unwrap(), [second]);
        assert_eq!(
            store
                .vault_request_get(RequestId::from_bytes([9; 16]))
                .unwrap(),
            None
        );
    }
}
