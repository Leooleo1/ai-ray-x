//! Images attached by the person in a bot's chat.
//! Validated by their actual file bytes (never by filename or declared MIME),
//! capped in count and size, stored under `<state>/uploads/<bot>/<id>.<ext>`,
//! and handed over to the CLI through `preview::hand_over` (`ai_dir`).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{bail, Context, Result};
use base64::Engine;
use serde_json::{json, Value};

use crate::room;

/// Maximum images attached to a single message.
pub const MAX_IMAGES: usize = 4;
/// Maximum decoded size of one image (5 MiB).
pub const MAX_BYTES: usize = 5 * 1024 * 1024;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
    Webp,
    Gif,
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Webp => "webp",
            Format::Gif => "gif",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Format::Png => "image/png",
            Format::Jpeg => "image/jpeg",
            Format::Webp => "image/webp",
            Format::Gif => "image/gif",
        }
    }
}

/// An image decoded and checked, ready to be written to disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validated {
    pub bytes: Vec<u8>,
    pub format: Format,
}

/// An uploaded image stored on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub name: String,
    pub path: PathBuf,
}

/// Identifies an allowed image format strictly from its magic bytes.
pub fn detect(bytes: &[u8]) -> Option<Format> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some(Format::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(Format::Jpeg)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(Format::Gif)
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(Format::Webp)
    } else {
        None
    }
}

/// Decodes and validates a list of base64 strings (with or without a `data:...;base64,` prefix).
/// Fails as a whole if any image is invalid, too large, or exceeds `MAX_IMAGES`.
pub fn validate_all(raw: &[String]) -> Result<Vec<Validated>> {
    if raw.len() > MAX_IMAGES {
        bail!("at most {MAX_IMAGES} images per message");
    }
    let mut out = Vec::with_capacity(raw.len());
    for entry in raw {
        out.push(validate_one(entry)?);
    }
    Ok(out)
}

fn validate_one(entry: &str) -> Result<Validated> {
    let trimmed = entry.trim();
    let encoded = if let Some(rest) = trimmed.strip_prefix("data:") {
        let Some((_, data)) = rest.split_once(";base64,") else {
            bail!("invalid image encoding");
        };
        data.trim()
    } else {
        trimmed
    };
    // Reject obviously oversized base64 payloads before decoding.
    if encoded.len() > MAX_BYTES * 2 {
        bail!("each image is at most 5 MB");
    }
    let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).context("invalid image encoding")?;
    if bytes.is_empty() {
        bail!("empty image");
    }
    if bytes.len() > MAX_BYTES {
        bail!("each image is at most 5 MB");
    }
    let format = detect(&bytes).context("unsupported image format; use PNG, JPEG, WebP or GIF")?;
    Ok(Validated { bytes, format })
}

/// Where a bot's uploaded images live: `<state>/uploads/<bot>`.
pub fn dir(state_dir: &Path, bot: &str) -> PathBuf {
    state_dir.join("uploads").join(bot)
}

/// Saves validated images under `<state>/uploads/<bot>/<id>.<ext>`.
pub fn save(state_dir: &Path, bot: &str, images: &[Validated]) -> Result<Vec<Saved>> {
    if images.is_empty() {
        return Ok(Vec::new());
    }
    let folder = dir(state_dir, bot);
    fs::create_dir_all(&folder)?;
    let mut saved = Vec::with_capacity(images.len());
    for image in images {
        let seq = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let name = format!("{:x}-{:04x}.{}", room::now_ms(), seq & 0xffff, image.format.ext());
        let path = folder.join(&name);
        if let Err(error) = fs::write(&path, &image.bytes) {
            remove(&saved);
            return Err(error.into());
        }
        saved.push(Saved { name, path });
    }
    Ok(saved)
}

/// Deletes previously saved images (used to roll back when starting a turn fails).
pub fn remove(saved: &[Saved]) {
    for item in saved {
        let _ = fs::remove_file(&item.path);
    }
}

/// Checks an upload filename and returns its MIME type when safe to read from the bot's folder.
fn mime_for_name(name: &str) -> Option<&'static str> {
    let (stem, ext) = name.split_once('.')?;
    if stem.is_empty() || stem.len() > 40 || !stem.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        return None;
    }
    match ext {
        "png" => Some("image/png"),
        "jpg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

/// A saved uploaded image of a bot, as a data URL for the panel (only files in its own folder).
pub fn image(state_dir: &Path, bot: &str, name: &str) -> Result<Value> {
    let Some(mime) = mime_for_name(name) else {
        bail!("no such image");
    };
    let bytes = fs::read(dir(state_dir, bot).join(name)).context("no such image")?;
    Ok(json!({ "image": format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)) }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    fn png_bytes() -> Vec<u8> {
        vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13]
    }

    fn jpeg_bytes() -> Vec<u8> {
        vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F']
    }

    fn webp_bytes() -> Vec<u8> {
        b"RIFF\x24\x00\x00\x00WEBPVP8 ".to_vec()
    }

    fn gif_bytes(version: &[u8; 6]) -> Vec<u8> {
        let mut out = version.to_vec();
        out.extend_from_slice(&[1, 0, 1, 0]);
        out
    }

    #[test]
    fn detects_allowed_formats_by_magic_bytes_only() {
        assert_eq!(detect(&png_bytes()), Some(Format::Png));
        assert_eq!(detect(&jpeg_bytes()), Some(Format::Jpeg));
        assert_eq!(detect(&webp_bytes()), Some(Format::Webp));
        assert_eq!(detect(&gif_bytes(b"GIF87a")), Some(Format::Gif));
        assert_eq!(detect(&gif_bytes(b"GIF89a")), Some(Format::Gif));

        // Disallowed formats and truncated headers are rejected.
        assert_eq!(detect(b""), None);
        assert_eq!(detect(b"<svg xmlns='http://www.w3.org/2000/svg'/>"), None);
        assert_eq!(detect(b"BM\x00\x00\x00\x00"), None);
        assert_eq!(detect(b"RIFF\x00\x00\x00\x00WAVE"), None);
        assert_eq!(detect(&[0x89, b'P', b'N', b'G']), None);
    }

    #[test]
    fn ignores_claimed_data_url_mime_and_validates_actual_bytes() {
        // Claiming image/png while sending SVG bytes must fail.
        let fake_png = format!("data:image/png;base64,{}", encode(b"<svg></svg>"));
        assert!(validate_all(&[fake_png]).is_err());

        // Claiming image/jpeg while sending actual PNG bytes detects PNG.
        let mislabeled = format!("data:image/jpeg;base64,{}", encode(&png_bytes()));
        let validated = validate_all(&[mislabeled]).unwrap();
        assert_eq!(validated[0].format, Format::Png);

        // Raw base64 without a data URL prefix also works.
        let raw = encode(&webp_bytes());
        let validated = validate_all(&[raw]).unwrap();
        assert_eq!(validated[0].format, Format::Webp);

        // Non-base64 data URL is rejected.
        assert!(validate_all(&["data:image/png,notbase64".to_string()]).is_err());
    }

    #[test]
    fn enforces_maximum_image_size_and_non_empty() {
        assert!(validate_all(&[encode(b"")]).is_err());

        let mut at_limit = png_bytes();
        at_limit.resize(MAX_BYTES, 0);
        let ok = validate_all(&[encode(&at_limit)]).unwrap();
        assert_eq!(ok[0].bytes.len(), MAX_BYTES);

        let mut over_limit = png_bytes();
        over_limit.resize(MAX_BYTES + 1, 0);
        let err = validate_all(&[encode(&over_limit)]).unwrap_err();
        assert!(err.to_string().contains("5 MB"));
    }

    #[test]
    fn enforces_maximum_image_count() {
        assert!(validate_all(&[]).unwrap().is_empty());

        let four: Vec<String> = (0..MAX_IMAGES).map(|_| encode(&jpeg_bytes())).collect();
        assert_eq!(validate_all(&four).unwrap().len(), MAX_IMAGES);

        let five: Vec<String> = (0..=MAX_IMAGES).map(|_| encode(&jpeg_bytes())).collect();
        let err = validate_all(&five).unwrap_err();
        assert!(err.to_string().contains(&format!("at most {MAX_IMAGES}")));
    }

    #[test]
    fn saves_under_uploads_folder_and_serves_safely() {
        let state = std::env::temp_dir().join(format!("raiox-uploads-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&state);

        let items = validate_all(&[encode(&png_bytes()), encode(&gif_bytes(b"GIF89a"))]).unwrap();
        let saved = save(&state, "bot1", &items).unwrap();
        assert_eq!(saved.len(), 2);
        assert!(saved[0].name.ends_with(".png"));
        assert!(saved[1].name.ends_with(".gif"));
        assert_ne!(saved[0].name, saved[1].name);
        assert_eq!(saved[0].path, state.join("uploads").join("bot1").join(&saved[0].name));
        assert!(saved[0].path.exists());

        let loaded = image(&state, "bot1", &saved[0].name).unwrap();
        assert!(loaded["image"].as_str().unwrap().starts_with("data:image/png;base64,"));

        // Path traversal and disallowed extensions are rejected.
        assert!(image(&state, "bot1", "../secret.png").is_err());
        assert!(image(&state, "bot1", "a/b.png").is_err());
        assert!(image(&state, "bot1", "1234.svg").is_err());
        assert!(image(&state, "bot1", "1234.png.jpg").is_err());

        remove(&saved);
        assert!(!saved[0].path.exists());
        let _ = fs::remove_dir_all(&state);
    }
}
