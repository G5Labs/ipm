//! Smart-HTTP mechanics; admission, ownership and publication remain API policy.
use super::{
    invalid, Context, FailureEffect, FailureKind, Invocation, NativeGit, NativeGitError,
    NativeGitStream,
};
use crate::GitAuth;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitService {
    UploadPack,
    ReceivePack,
}
impl GitService {
    fn command(self) -> &'static str {
        match self {
            Self::UploadPack => "upload-pack",
            Self::ReceivePack => "receive-pack",
        }
    }
}

/// The exact command authorized by the caller. All-zero old/new IDs retain
/// Git's creation/deletion meanings; authorization for either stays with policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiveRefUpdate {
    pub reference: String,
    pub old_oid: String,
    pub new_oid: String,
    /// Policy admits only a fast-forward. Git enforces it inside its locked
    /// ref transaction, since the new commit arrives in the pack. One push
    /// either requests it for every command or for none.
    pub fast_forward_only: bool,
}

impl ReceiveRefUpdate {
    fn same_command(&self, other: &Self) -> bool {
        self.reference == other.reference
            && self.old_oid == other.old_oid
            && self.new_oid == other.new_oid
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiveRefStatus {
    pub reference: String,
    pub accepted: bool,
    pub detail: Option<String>,
}
#[derive(Debug)]
pub struct ReceivePackResult {
    pub response: Vec<u8>,
    pub statuses: Vec<ReceiveRefStatus>,
    /// Whether Git emitted a report (including an unpack/refusal report).
    pub has_report: bool,
    /// Atomic updates were requested; otherwise commands may succeed separately.
    pub atomic: bool,
    /// A process failure can coexist with already confirmed ref acceptances.
    pub failure: Option<NativeGitError>,
}

impl NativeGit {
    pub async fn advertise_refs(
        &self,
        repository: &Path,
        service: GitService,
    ) -> Result<Vec<u8>, NativeGitError> {
        let operation = "advertise refs";
        let result = async {
            let mut command = self.protocol_command(repository, operation, service, true)?;
            command.read_only().arg("--advertise-refs");
            command.arg(
                repository
                    .canonicalize()
                    .map_err(|_| invalid(operation, "Cannot resolve repository storage"))?,
            );
            let output = command.execute().await?;
            if !output.completed_successfully() {
                return Err(output.failure(
                    operation,
                    &GitAuth::None,
                    FailureKind::Exit,
                    FailureEffect::Refused,
                ));
            }
            Ok(output.into_protocol_bytes())
        }
        .await;
        result.map_err(|error: NativeGitError| {
            error.at_target(&repository.to_string_lossy(), &GitAuth::None)
        })
    }

    pub async fn upload_pack(
        &self,
        repository: &Path,
        request: Vec<u8>,
    ) -> Result<NativeGitStream, NativeGitError> {
        let operation = "upload-pack";
        let result = async {
            let mut command =
                self.protocol_command(repository, operation, GitService::UploadPack, false)?;
            command
                .read_only()
                .arg(
                    repository
                        .canonicalize()
                        .map_err(|_| invalid(operation, "Cannot resolve repository storage"))?,
                )
                .input(request);
            command.execute_stream(repository).await
        }
        .await;
        result.map_err(|error: NativeGitError| {
            error.at_target(&repository.to_string_lossy(), &GitAuth::None)
        })
    }

    /// Execute exactly the admitted ref commands. Git enforces their old-OID CAS
    /// and requested atomic/per-ref behavior. This does not acquire authority.
    pub async fn receive_pack(
        &self,
        repository: &Path,
        request: Vec<u8>,
        admitted: &[ReceiveRefUpdate],
    ) -> Result<ReceivePackResult, NativeGitError> {
        let operation = "receive-pack";
        let result = async {
            let (actual, shallow, atomic) = parse_commands(&request)?;
            if actual.len() != admitted.len()
                || actual
                    .iter()
                    .zip(admitted)
                    .any(|(actual, admitted)| !actual.same_command(admitted))
            {
                return Err(invalid(
                    operation,
                    "Receive commands do not match the admitted ref vector",
                ));
            }
            let fast_forward_only = admitted.iter().any(|update| update.fast_forward_only);
            if fast_forward_only && !admitted.iter().all(|update| update.fast_forward_only) {
                return Err(invalid(
                    operation,
                    "A push cannot mix fast-forward-only and rewritable commands",
                ));
            }
            for update in admitted {
                self.check_full_ref(&update.reference, operation).await?;
            }
            let oids: Vec<_> = admitted
                .iter()
                .flat_map(|update| [update.old_oid.as_str(), update.new_oid.as_str()])
                .chain(shallow.iter().map(String::as_str))
                .collect();
            if !oids.is_empty() {
                self.check_object_format(repository, &oids, operation)
                    .await?;
            }
            let mut command =
                self.protocol_command(repository, operation, GitService::ReceivePack, true)?;
            // Admission permits workspace rewrites unless policy asked for a
            // fast-forward. Git's old-OID CAS is still definitive, and provider
            // publication keeps its independent policy.
            command.config(
                "receive.denyNonFastForwards",
                if fast_forward_only { "true" } else { "false" },
            );
            if repository.join(".git").exists() {
                command.config("receive.denyCurrentBranch", "updateInstead");
            }
            command
                .arg(
                    repository
                        .canonicalize()
                        .map_err(|_| invalid(operation, "Cannot resolve repository storage"))?,
                )
                .input(request);
            let output = command.execute_captured().await?;
            let report = report_lines(output.protocol_bytes());
            let statuses: Vec<_> = admitted
                .iter()
                .filter_map(|update| {
                    if report
                        .iter()
                        .any(|line| line == &format!("ok {}", update.reference))
                    {
                        Some(ReceiveRefStatus {
                            reference: update.reference.clone(),
                            accepted: true,
                            detail: None,
                        })
                    } else {
                        report
                            .iter()
                            .find_map(|line| {
                                line.strip_prefix(&format!("ng {} ", update.reference))
                            })
                            .map(|detail| ReceiveRefStatus {
                                reference: update.reference.clone(),
                                accepted: false,
                                detail: Some(output.redact(detail)),
                            })
                    }
                })
                .collect();
            let failure = if output.completed_successfully() {
                None
            } else {
                let accepted: Vec<_> = statuses
                    .iter()
                    .filter(|status| status.accepted)
                    .map(|status| status.reference.as_str())
                    .collect();
                let mut error = output.failure(
                    operation,
                    &GitAuth::None,
                    FailureKind::Exit,
                    if accepted.is_empty() {
                        FailureEffect::Unknown
                    } else {
                        FailureEffect::Partial
                    },
                );
                if !accepted.is_empty() {
                    error.detail = format!(
                        "Git confirmed accepted refs {}; {}",
                        accepted.join(", "),
                        error.detail
                    );
                }
                Some(error.at_target(&repository.to_string_lossy(), &GitAuth::None))
            };
            Ok(ReceivePackResult {
                response: output.into_protocol_bytes(),
                statuses,
                has_report: !report.is_empty(),
                atomic,
                failure,
            })
        }
        .await;
        result.map_err(|error: NativeGitError| {
            error.at_target(&repository.to_string_lossy(), &GitAuth::None)
        })
    }

    fn protocol_command(
        &self,
        repository: &Path,
        operation: &'static str,
        service: GitService,
        buffered: bool,
    ) -> Result<Invocation, NativeGitError> {
        if !matches!(self.context, Context::Api) {
            return Err(invalid(
                operation,
                "Server Git protocols require the API execution context",
            ));
        }
        let root = repository
            .canonicalize()
            .map_err(|_| invalid(operation, "Cannot resolve repository storage"))?;
        let dot_git = root.join(".git");
        let git_dir = if dot_git.exists() {
            dot_git
        } else {
            root.clone()
        };
        let mut command = self.command(Some(&root), operation);
        command.server_repository(&root, &git_dir);
        for reference in &self.hidden_protocol_refs {
            command.config("uploadpack.hideRefs", reference);
            command.config("receive.hideRefs", reference);
        }
        if !self.hidden_protocol_refs.is_empty() {
            for key in [
                "uploadpack.allowAnySHA1InWant",
                "uploadpack.allowReachableSHA1InWant",
                "uploadpack.allowTipSHA1InWant",
            ] {
                command.config(key, "false");
            }
        }
        command.args([service.command(), "--stateless-rpc"]);
        if buffered {
            command.protocol_output();
        }
        Ok(command)
    }
}

fn parse_commands(
    request: &[u8],
) -> Result<(Vec<ReceiveRefUpdate>, Vec<String>, bool), NativeGitError> {
    let operation = "receive-pack";
    let mut rest = request;
    let mut updates = Vec::new();
    let mut shallow = Vec::new();
    let mut atomic = false;
    loop {
        let length = rest
            .get(..4)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .and_then(|value| usize::from_str_radix(value, 16).ok())
            .ok_or_else(|| invalid(operation, "Invalid receive command framing"))?;
        if length == 0 {
            return Ok((updates, shallow, atomic));
        }
        if length < 4 || length > rest.len() {
            return Err(invalid(operation, "Truncated receive command"));
        }
        let line = std::str::from_utf8(&rest[4..length])
            .map_err(|_| invalid(operation, "Invalid receive command encoding"))?
            .trim_end_matches('\n');
        rest = &rest[length..];
        if let Some(oid) = line.strip_prefix("shallow ") {
            if !updates.is_empty() || !full_oid(oid) {
                return Err(invalid(operation, "Invalid shallow boundary"));
            }
            shallow.push(oid.to_owned());
            continue;
        }
        let (line, capabilities) = line
            .split_once('\0')
            .map_or((line, None), |(line, caps)| (line, Some(caps)));
        if let Some(capabilities) = capabilities {
            if !updates.is_empty() {
                return Err(invalid(
                    operation,
                    "Capabilities must precede receive updates",
                ));
            }
            atomic = capabilities
                .split_whitespace()
                .any(|capability| capability == "atomic");
        }
        let fields: Vec<_> = line.split(' ').collect();
        if fields.len() != 3
            || !full_oid(fields[0])
            || !full_oid(fields[1])
            || !fields[2].starts_with("refs/")
            || fields[2].contains('\0')
        {
            return Err(invalid(operation, "Invalid receive ref command"));
        }
        if updates
            .iter()
            .any(|update: &ReceiveRefUpdate| update.reference == fields[2])
        {
            return Err(invalid(operation, "Duplicate receive ref command"));
        }
        updates.push(ReceiveRefUpdate {
            old_oid: fields[0].into(),
            new_oid: fields[1].into(),
            reference: fields[2].into(),
            fast_forward_only: false,
        });
    }
}
fn full_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn report_lines(output: &[u8]) -> Vec<String> {
    fn packets(mut data: &[u8]) -> Vec<&[u8]> {
        let mut result = Vec::new();
        while data.len() >= 4 {
            let Some(len) = std::str::from_utf8(&data[..4])
                .ok()
                .and_then(|len| usize::from_str_radix(len, 16).ok())
            else {
                break;
            };
            if len < 4 || len > data.len() {
                break;
            }
            result.push(&data[4..len]);
            data = &data[len..];
        }
        result
    }
    let outer = packets(output);
    let banded = outer.iter().any(|packet| packet.first() == Some(&1));
    let unbanded: Vec<_> = outer
        .iter()
        .filter(|packet| packet.first() == Some(&1))
        .flat_map(|packet| packet[1..].iter().copied())
        .collect();
    let lines = if banded { packets(&unbanded) } else { outer };
    lines
        .into_iter()
        .map(|line| String::from_utf8_lossy(line).trim().to_string())
        .collect()
}
