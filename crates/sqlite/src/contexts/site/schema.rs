//! Tables of the site context (migrations/site).

diesel::table! {
    site_states (session_id, origin) {
        session_id -> Text,
        origin -> Text,
        state -> Text,
        item_name -> Nullable<Text>,
        checked_at -> Text,
        recorded_by -> Text,
    }
}
