#![allow(non_snake_case)]
//! IPM package loading and transaction history (stage B1 of the IPM Verus uplift).
//!
//! - [`model`]: L2, the as-built machine at 571e04a1 over a model of the IRIS process.
//! - [`contract`]: L1, the required outcomes and the history contract, with open questions as
//!   parameters.
//! - [`proofs`]: invariants of L2 and the refinement obligations that hold.
//! - [`cases`]: per-case lemmas (as-built observation and L1 verdict), proved by evaluation.
//! - [`explore`]: explorer harness and case generator over the executable twin.
pub mod model;
pub mod contract;
pub mod explore;
pub mod cases;
pub mod proofs;
