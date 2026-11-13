//! Git credential and worker branch helpers used by current CLI/worker operations.

/// Source-compatible nested credential module retained for existing library
/// consumers while the implementation lives in `g5d_foundation::scm_credentials`.
pub mod credentials {
    pub use g5d_foundation::scm_credentials::*;
}

pub use credentials::ScmCredential;
