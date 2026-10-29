//! Tables of the request context (migrations/request).

diesel::table! {
    vault_requests (id) {
        id -> Text,
        agent -> Text,
        kind -> Text,
        name -> Text,
        reason -> Text,
        origin -> Nullable<Text>,
        status -> Text,
        created_at -> BigInt,
        item_name -> Nullable<Text>,
    }
}
