//! Tables of the config context (migrations/config).

diesel::table! {
    settings (key) {
        key -> Text,
        value -> Text,
    }
}
