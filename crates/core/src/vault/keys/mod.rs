//! The vault's keys: creating and opening it ([`open`], which finishes an interrupted root
//! rotation), moving pre-R-40 items into the developer domain ([`developer`]) and rotating
//! root and domain keys ([`rotate`], 07 §7.8).

mod developer;
mod open;
mod rotate;
