//! What every attachment adapter shares (`g5.attachments`): the kinds an
//! attachment can be and recognizing an image or video by its content.

use serde::Serialize;

/// What an attachment is, as a listing names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AttachmentKind {
    Image,
    Video,
    File,
}

impl AttachmentKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
            Self::File => "file",
        }
    }
}

/// Bytes of content enough to recognize any supported image or video.
pub const RECOGNITION_PREFIX: usize = 16;

/// The image type `bytes` begin with: PNG, JPEG, GIF or WebP. SVG and every
/// other format are not recognized.
#[must_use]
pub fn image_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// The video type `bytes` begin with: MP4, QuickTime or WebM.
#[must_use]
pub fn video_type(bytes: &[u8]) -> Option<&'static str> {
    // ISO media files also carry still images (HEIF, AVIF) and audio
    // (M4A); their major brands are not videos.
    const NOT_VIDEO: [&[u8; 4]; 10] = [
        b"heic", b"heix", b"heim", b"heis", b"mif1", b"msf1", b"avif", b"avis", b"M4A ", b"M4B ",
    ];
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        if NOT_VIDEO.iter().any(|brand| &bytes[8..12] == *brand) {
            return None;
        }
        Some(if &bytes[8..12] == b"qt  " {
            "video/quicktime"
        } else {
            "video/mp4"
        })
    } else if bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        Some("video/webm")
    } else {
        None
    }
}

/// The usual file extension for a recognized type, for naming saved files.
#[must_use]
pub fn extension(content_type: &str) -> Option<&'static str> {
    Some(match content_type {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "video/mp4" => "mp4",
        "video/quicktime" => "mov",
        "video/webm" => "webm",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_decides_the_type() {
        assert_eq!(image_type(b"\x89PNG\r\n\x1a\nrest"), Some("image/png"));
        assert_eq!(image_type(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(image_type(b"GIF89a..."), Some("image/gif"));
        assert_eq!(image_type(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(
            image_type(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
            None
        );
        assert_eq!(image_type(b"<!doctype html>"), None);
        assert_eq!(image_type(b""), None);
        assert_eq!(video_type(b"\0\0\0\x18ftypmp42\0\0"), Some("video/mp4"));
        assert_eq!(
            video_type(b"\0\0\0\x14ftypqt  \0\0"),
            Some("video/quicktime")
        );
        assert_eq!(video_type(&[0x1A, 0x45, 0xDF, 0xA3, 1]), Some("video/webm"));
        assert_eq!(video_type(b"\x89PNG\r\n\x1a\n...."), None);
        assert_eq!(video_type(b"\0\0\0\x18ftypheic\0\0"), None);
        assert_eq!(video_type(b"\0\0\0\x1cftypavif\0\0"), None);
        assert_eq!(video_type(b"\0\0\0\x20ftypM4A \0\0"), None);
        assert_eq!(extension("video/quicktime"), Some("mov"));
        assert_eq!(extension("application/pdf"), None);
    }
}
