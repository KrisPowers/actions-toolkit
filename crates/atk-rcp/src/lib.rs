//! RCP ("Run Control Protocol"): the transport shells use to talk back to their owning bucket
//! instead of touching the database directly. This crate only provides the generic framing and
//! local-transport plumbing; the actual request/response message types live in `core`, which is
//! the only place that needs both this crate and `atk-db`.

pub mod framing;
pub mod local;

pub use local::{connect, endpoint_for_bucket, LocalListener};
