//! Deterministic host qualification suites (K1 / G5). They live in the
//! crate's own test tree so the existing lib-test lanes run them without a
//! new Docker or Cloud lane.

mod control_session;
mod lifecycle;
