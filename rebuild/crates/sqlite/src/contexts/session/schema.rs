//! Tables of the session context (migrations/session).

diesel::table! {
    sessions (id) {
        id -> Text,
        name -> Text,
        state -> Text,
    }
}

diesel::table! {
    generations (session_id) {
        session_id -> Text,
        generation -> Text,
        cdp -> Bool,
    }
}

diesel::table! {
    handoff_requests (request_id) {
        request_id -> Text,
        session_id -> Text,
        generation -> Text,
        origin -> Nullable<Text>,
        status -> Text,
    }
}

diesel::joinable!(handoff_requests -> sessions (session_id));
diesel::allow_tables_to_appear_in_same_query!(sessions, generations, handoff_requests);
