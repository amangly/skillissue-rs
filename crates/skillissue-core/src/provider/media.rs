//! Inline media is validated before discovery or any authenticated upload.

use base64::{Engine, engine::general_purpose::STANDARD};
use std::io::Cursor;

const HOSTED_IMAGE_BYTES: usize = 4 * 1024 * 1024;
const HOSTED_AUDIO_BYTES: usize = 2 * 1024 * 1024;

pub(super) fn validate_hosted_image(data_url: &str) -> Result<(), String> {
    let invalid = || {
        "Palepale screenshots require a valid inline PNG or JPEG of at most 4 MiB and 4096 pixels per dimension.".to_string()
    };
    let (prefix, encoded) = data_url.split_once(',').ok_or_else(invalid)?;
    if !matches!(prefix, "data:image/png;base64" | "data:image/jpeg;base64")
        || encoded.is_empty()
        || encoded.len() > HOSTED_IMAGE_BYTES.div_ceil(3) * 4
    {
        return Err(invalid());
    }
    let bytes = STANDARD.decode(encoded).map_err(|_| invalid())?;
    if bytes.len() > HOSTED_IMAGE_BYTES {
        return Err(invalid());
    }
    let (width, height) = if prefix == "data:image/png;base64" {
        if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err(invalid());
        }
        image::ImageReader::with_format(Cursor::new(&bytes), image::ImageFormat::Png)
            .into_dimensions()
            .map_err(|_| invalid())?
    } else {
        jpeg_dimensions(&bytes).ok_or_else(invalid)?
    };
    if width == 0 || height == 0 || width > 4096 || height > 4096 {
        return Err(invalid());
    }
    Ok(())
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(&[0xff, 0xd8]) || !bytes.ends_with(&[0xff, 0xd9]) {
        return None;
    }
    let mut offset = 2;
    while offset < bytes.len() {
        if *bytes.get(offset)? != 0xff {
            return None;
        }
        while *bytes.get(offset)? == 0xff {
            offset += 1;
        }
        let marker = *bytes.get(offset)?;
        offset += 1;
        if matches!(marker, 0xd9 | 0xda) {
            return None;
        }
        let length = u16::from_be_bytes([*bytes.get(offset)?, *bytes.get(offset + 1)?]) as usize;
        if length < 2 || offset.checked_add(length)? > bytes.len() {
            return None;
        }
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            if length < 8 {
                return None;
            }
            let height = u16::from_be_bytes([bytes[offset + 3], bytes[offset + 4]]) as u32;
            let width = u16::from_be_bytes([bytes[offset + 5], bytes[offset + 6]]) as u32;
            return Some((width, height));
        }
        offset += length;
    }
    None
}

pub(super) fn validate_hosted_wav(wav: &[u8]) -> Result<(), String> {
    let invalid = || {
        "Palepale audio requires a complete PCM16 WAV, mono, at 16, 22.05, 24, 44.1 or 48 kHz, at most 30 seconds and 2 MiB.".to_string()
    };
    if wav.len() < 44
        || wav.len() > HOSTED_AUDIO_BYTES
        || &wav[..4] != b"RIFF"
        || &wav[8..12] != b"WAVE"
        || u32::from_le_bytes(wav[4..8].try_into().map_err(|_| invalid())?) as usize + 8
            != wav.len()
    {
        return Err(invalid());
    }
    let mut reader = hound::WavReader::new(Cursor::new(wav)).map_err(|_| invalid())?;
    let spec = reader.spec();
    if spec.sample_format != hound::SampleFormat::Int
        || spec.bits_per_sample != 16
        || spec.channels != 1
        || ![16_000, 22_050, 24_000, 44_100, 48_000].contains(&spec.sample_rate)
        || reader.duration() == 0
        || reader.duration() > spec.sample_rate * 30
    {
        return Err(invalid());
    }
    for sample in reader.samples::<i16>() {
        sample.map_err(|_| invalid())?;
    }
    Ok(())
}
