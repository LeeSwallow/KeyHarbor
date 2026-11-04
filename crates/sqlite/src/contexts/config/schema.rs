//! Tables of the config context (migrations/config).

diesel::table! {
    settings (key) {
        key -> Text,
        value -> Text,
    }
}

diesel::table! {
    credential_templates (name) {
        name -> Text,
        fields -> Text,
    }
}
