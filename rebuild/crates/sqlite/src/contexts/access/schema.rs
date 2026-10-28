//! Tables of the access context (migrations/access).

diesel::table! {
    policy_rules (agent, resource, target) {
        agent -> Text,
        resource -> Text,
        target -> Text,
        effect -> Text,
    }
}
