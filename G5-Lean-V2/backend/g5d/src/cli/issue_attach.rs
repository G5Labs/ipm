//! `--attach` on `g5d issue create`, `comment` and `edit`: images from local
//! files added to a G5 issue as the owner and shown at the end of the text
//! (`g5.attachments.cli.attach`).
//!
//! Files are checked before they are read whole and uploaded before the text
//! that shows them is sent, so a failure leaves at most attachments listed
//! at the issue. The server keys each upload by its issue, name and content,
//! so a rerun adds none twice.

use std::path::{Path, PathBuf};

use g5_contracts::error::G5Error;
use g5_internal_api::client::InternalApiClient;
use serde_json::Value;

/// Most files one command attaches.
pub(super) const MAX_FILES: usize = 10;
/// Largest image: the native limit (10 MiB).
pub(super) const MAX_BYTES: u64 = 10 * 1024 * 1024;

/// An image read from a local file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LocalImage {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Images uploaded to one issue.
#[derive(Debug, Clone, Default)]
pub(super) struct Uploaded {
    /// The Markdown that shows each, in order.
    pub markdown: Vec<String>,
    /// The issue's page: later writes name the issue the images are on.
    pub issue_url: String,
}

fn invalid(message: String) -> G5Error {
    G5Error::ValidationError {
        details: serde_json::json!({"field": "attach", "error": message}),
    }
}

/// Read the `--attach` files: at most [`MAX_FILES`] regular files of at most
/// [`MAX_BYTES`], each a PNG, JPEG, GIF or WebP image.
pub(super) fn read_images(paths: &[PathBuf]) -> Result<Vec<LocalImage>, G5Error> {
    if paths.len() > MAX_FILES {
        return Err(invalid(format!(
            "attach at most {MAX_FILES} files per command"
        )));
    }
    paths.iter().map(|path| read_image(path)).collect()
}

fn read_image(path: &Path) -> Result<LocalImage, G5Error> {
    let shown = path.display();
    let metadata =
        std::fs::metadata(path).map_err(|e| invalid(format!("cannot read {shown}: {e}")))?;
    if !metadata.is_file() {
        return Err(invalid(format!("{shown} is not a regular file")));
    }
    if metadata.len() == 0 || metadata.len() > MAX_BYTES {
        return Err(invalid(format!(
            "{shown} is {} bytes; an image must be 1 to {MAX_BYTES} bytes",
            metadata.len()
        )));
    }
    // Read through the handle that was checked, never more than the bound,
    // even if the path was replaced since.
    let file =
        std::fs::File::open(path).map_err(|e| invalid(format!("cannot read {shown}: {e}")))?;
    if !file.metadata().is_ok_and(|opened| opened.is_file()) {
        return Err(invalid(format!("{shown} is not a regular file")));
    }
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(file, MAX_BYTES + 1), &mut bytes)
        .map_err(|e| invalid(format!("cannot read {shown}: {e}")))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid(format!("{shown} grew past {MAX_BYTES} bytes")));
    }
    if g5_contracts::attachments::image_type(&bytes).is_none() {
        return Err(invalid(format!(
            "{shown} is not a PNG, JPEG, GIF or WebP image"
        )));
    }
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "image".to_string());
    Ok(LocalImage { name, bytes })
}

/// Upload `images` to the issue `reference` names (the primary issue when
/// `None`), each to the issue the first went to.
pub(super) async fn upload(
    client: &InternalApiClient,
    workspace_id: &str,
    reference: Option<&str>,
    images: &[LocalImage],
) -> Result<Uploaded, G5Error> {
    let mut uploaded = Uploaded::default();
    for (index, image) in images.iter().enumerate() {
        let target = if index == 0 {
            reference
        } else {
            Some(uploaded.issue_url.as_str())
        };
        let response = client
            .upload_issue_attachment(workspace_id, target, &image.name, &image.bytes)
            .await
            .inspect_err(|_| {
                if index > 0 {
                    eprintln!(
                        "{} of {} images were added to {} and are listed at the issue; \
                         rerun to add the rest without adding those twice.",
                        index,
                        images.len(),
                        uploaded.issue_url
                    );
                }
            })?;
        let data = &response["data"];
        if index == 0 {
            uploaded.issue_url = text(&data["issueUrl"]).to_string();
        }
        uploaded
            .markdown
            .push(text(&data["attachment"]["markdown"]).to_string());
    }
    Ok(uploaded)
}

/// `text` with each image's Markdown after it, one per paragraph.
pub(super) fn with_images(text: &str, markdown: &[String]) -> String {
    let text = text.trim_end();
    let images = markdown.join("\n\n");
    if text.is_empty() {
        images
    } else {
        format!("{text}\n\n{images}")
    }
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_follow_the_text_one_per_paragraph() {
        let md = vec!["![a.png](u/1)".to_string(), "![b.gif](u/2)".to_string()];
        assert_eq!(
            with_images("Looks off:\n", &md),
            "Looks off:\n\n![a.png](u/1)\n\n![b.gif](u/2)"
        );
        assert_eq!(with_images("  \n", &md[..1]), "![a.png](u/1)");
    }

    #[test]
    fn only_bounded_regular_image_files_are_read() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("shot.png");
        std::fs::write(&png, b"\x89PNG\r\n\x1a\nimage").unwrap();
        let text = dir.path().join("notes.txt");
        std::fs::write(&text, b"plain").unwrap();
        let empty = dir.path().join("empty.png");
        std::fs::write(&empty, b"").unwrap();
        let big = dir.path().join("big.png");
        let file = std::fs::File::create(&big).unwrap();
        file.set_len(MAX_BYTES + 1).unwrap();

        let read = read_images(std::slice::from_ref(&png)).unwrap();
        assert_eq!(read[0].name, "shot.png");
        for refused in [&text, &empty, &big, &dir.path().to_path_buf()] {
            assert!(
                read_images(std::slice::from_ref(refused)).is_err(),
                "{refused:?}"
            );
        }
        assert!(read_images(&vec![png; MAX_FILES + 1]).is_err());
    }
}
