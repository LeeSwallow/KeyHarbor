//! Session context: sessions, the generation of each run, and authentication handoff
//! requests. Implements `SessionStore`; every state change is a compare-and-swap on the
//! session's current generation.

mod mappers;
mod models;
mod repo;
mod schema;
