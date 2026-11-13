//! The VM CLI requests transport from its local trusted runtime. It does not
//! read a credential, invoke network Git or choose an external destination.

use g5_contracts::{
    workspace_git::{LocalGitRequest, SOCKET_ENV},
    G5Error,
};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

pub async fn execute(request: LocalGitRequest) -> Result<(), G5Error> {
    let path = std::env::var_os(SOCKET_ENV).ok_or_else(|| {
        G5Error::Config("This command requires a running G5 workspace runtime".into())
    })?;
    let result = tokio::time::timeout(std::time::Duration::from_secs(335), async {
        let mut stream = tokio::net::UnixStream::connect(path).await?;
        let mut bytes = serde_json::to_vec(&request)?;
        bytes.push(b'\n');
        stream.write_all(&bytes).await?;
        let mut reader = BufReader::new(stream.take(2 * 1024 * 1024));
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        let result: Value = serde_json::from_str(&line)?;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(result)
    })
    .await
    .map_err(|_| G5Error::ServiceUnavailable {
        message: "Workspace ref refresh timed out".into(),
    })?
    .map_err(|error| G5Error::ServiceUnavailable {
        message: format!("Workspace Git operation failed: {error}"),
    })?;
    println!(
        "{}",
        serde_json::to_string_pretty(&result).unwrap_or_default()
    );
    if result["status"] != "succeeded" {
        return Err(G5Error::Conflict {
            reason: "Workspace Git operation was refused; see the reported member results".into(),
        });
    }
    Ok(())
}
