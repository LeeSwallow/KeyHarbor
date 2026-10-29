//! Tables of the access context (migrations/access).

diesel::table! {
    policy_rules (agent, resource, target) {
        agent -> Text,
        resource -> Text,
        target -> Text,
        effect -> Text,
    }
}

diesel::table! {
    origin_uses (item_name, origin) {
        item_name -> Text,
        origin -> Text,
    }
}

diesel::table! {
    grants (id) {
        id -> Text,
        subject_kind -> Text,
        subject -> Text,
        scope_kind -> Text,
        scope -> Text,
        until -> Nullable<BigInt>,
        created_by -> Text,
        reason -> Nullable<Text>,
        created_at -> BigInt,
        revoked_at -> Nullable<BigInt>,
        cleared -> Bool,
    }
}
