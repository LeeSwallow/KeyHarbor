//! Vault context: the vault row, wrapped domain keys, items with their sealed fields and
//! origins. Implements `VaultStore`. Only ciphertext and non-secret metadata are stored.

mod mappers;
mod models;
mod repo;
mod schema;
