//! The CDP pipe: framing ([`protocol`]) and the request/response client ([`client`]).
//! Crate-private: only the browser adapter and the supervisor use it.

mod client;
mod protocol;

pub(crate) use client::Cdp;
