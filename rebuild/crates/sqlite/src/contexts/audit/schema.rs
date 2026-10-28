//! Tables of the audit context (migrations/audit).

diesel::table! {
    audit (seq) {
        seq -> BigInt,
        at -> Text,
        agent -> Text,
        session_id -> Nullable<Text>,
        kind -> Text,
        item_name -> Nullable<Text>,
        origin -> Nullable<Text>,
        result -> Text,
    }
}
