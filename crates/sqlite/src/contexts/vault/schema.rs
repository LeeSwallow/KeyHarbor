//! Tables of the vault context (migrations/vault).

diesel::table! {
    vault (id) {
        id -> Binary,
        kek_id -> Binary,
        version -> BigInt,
        singleton -> Integer,
    }
}

diesel::table! {
    domain_keys (domain) {
        domain -> Integer,
        dk_id -> Binary,
        wrapped -> Binary,
    }
}

diesel::table! {
    vault_items (id) {
        id -> Binary,
        name -> Text,
        domain -> Integer,
        kind -> Text,
        origin_binding -> Bool,
        status -> Text,
        source -> Text,
        meta_mac -> Binary,
        updated_at -> Text,
        run_allowed -> Bool,
        expires_at -> Nullable<Text>,
        description -> Nullable<Text>,
        tags -> Text,
    }
}

diesel::table! {
    item_field_descriptions (item_id, field) {
        item_id -> Binary,
        field -> Text,
        description -> Text,
    }
}

diesel::table! {
    vault_fields (item_id, field) {
        item_id -> Binary,
        field -> Text,
        sealed -> Binary,
        previous -> Nullable<Binary>,
    }
}

diesel::table! {
    item_origins (item_id, origin) {
        item_id -> Binary,
        origin -> Text,
    }
}

diesel::joinable!(vault_fields -> vault_items (item_id));
diesel::joinable!(item_origins -> vault_items (item_id));
diesel::joinable!(item_field_descriptions -> vault_items (item_id));
diesel::allow_tables_to_appear_in_same_query!(vault_fields, vault_items);
