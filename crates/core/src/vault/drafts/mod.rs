//! Drafts (08 §8.2–8.3): items an agent adds without seeing a value ([`register`]) and how a
//! person or a sign-in settles them ([`settle`]).

mod register;
mod settle;

pub use register::{DraftItem, GeneratedDraft, GeneratedLogin, generate_password};
