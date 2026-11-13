//! Preserve an operation's durable identity when COMMIT completion is unknown.
use g5_authz::{ScopedDb, ScopedDbError};
use g5_contracts::G5Error;

pub async fn commit_durable_scope(db: ScopedDb) -> Result<(), G5Error> {
    db.commit().await.map_err(durable_commit_error)
}

fn durable_commit_error(error: ScopedDbError) -> G5Error {
    if error.commit_was_refused() {
        G5Error::Internal {
            message: format!("Failed to commit operation database scope: {error}"),
        }
    } else {
        G5Error::ServiceUnavailable {
            message: format!(
                "Operation commit acknowledgement is uncertain; retry its durable identity: {error}"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definite_precommit_refusal_is_internal() {
        let error = ScopedDbError::ClearFailed(sqlx::Error::Protocol("clear refused".into()));
        assert!(matches!(
            durable_commit_error(error),
            G5Error::Internal { .. }
        ));
    }

    #[test]
    fn a_lost_commit_acknowledgement_requires_retrying_the_durable_identity() {
        let error = ScopedDbError::CommitFailed(sqlx::Error::Io(std::io::Error::from(
            std::io::ErrorKind::ConnectionReset,
        )));
        assert!(
            matches!(durable_commit_error(error), G5Error::ServiceUnavailable { message }
            if message.contains("retry its durable identity"))
        );
    }
}
