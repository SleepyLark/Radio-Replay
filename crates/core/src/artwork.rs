//! Bounded metadata inspection before granting artwork to the webview.
//! No raster data is decoded here. PNG: w3.org/TR/png/; GIF: GIF89a;
//! WebP: RFC 9649; JPEG: ISO/IEC 10918-1 marker segments.
use std::{fs::File, io::Read, path::Path};

pub const MAX_BYTES: u64 = 25 * 1024 * 1024;
pub const MAX_EDGE: u32 = 8192;
pub const MAX_PIXELS: u64 = 16 * 1024 * 1024;
const MAX_FRAMES: u32 = 256;
const MAX_ANIMATION_PIXELS: u64 = 64 * 1024 * 1024;
const INVALID: &str = "Artwork has invalid or unsupported image headers";
type Size = (u32, u32);

struct Bytes<'a> {
    data: &'a [u8],
    at: usize,
}
impl<'a> Bytes<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).ok_or(INVALID)?;
        let result = self.data.get(self.at..end).ok_or(INVALID)?;
        self.at = end;
        Ok(result)
    }
    fn byte(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
}
fn le16(b: &[u8]) -> u32 {
    u16::from_le_bytes([b[0], b[1]]) as u32
}
fn be16(b: &[u8]) -> u32 {
    u16::from_be_bytes([b[0], b[1]]) as u32
}
fn le24(b: &[u8]) -> u32 {
    u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16
}
fn le32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}
fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}
fn check((width, height): Size, frames: u32) -> Result<(), String> {
    let pixels = u64::from(width) * u64::from(height);
    if width == 0 || height == 0 || width > MAX_EDGE || height > MAX_EDGE || pixels > MAX_PIXELS {
        return Err("Artwork must be at most 8,192 pixels per side and 16 megapixels".into());
    }
    if frames > MAX_FRAMES || pixels * u64::from(frames) > MAX_ANIMATION_PIXELS {
        return Err(
            "Animated artwork exceeds the 256-frame or 64-megapixel total frame limit".into(),
        );
    }
    Ok(())
}
fn rectangle(canvas: Size, left: u32, top: u32, size: Size) -> Result<(), String> {
    check(size, 1)?;
    if u64::from(left) + u64::from(size.0) > u64::from(canvas.0)
        || u64::from(top) + u64::from(size.1) > u64::from(canvas.1)
    {
        return Err(INVALID.into());
    }
    Ok(())
}
pub fn validate(path: &Path) -> Result<(), String> {
    if !crate::library::image_file(path) {
        return Err("Choose PNG, JPEG, GIF or WebP artwork".into());
    }
    let file = File::open(path).map_err(|e| format!("Cannot read artwork: {e}"))?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("Choose an image file".into());
    }
    if metadata.len() > MAX_BYTES {
        return Err("Artwork exceeds the 25 MiB limit".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Artwork exceeds the 25 MiB limit".into());
    }
    inspect(&bytes)
}
fn inspect(data: &[u8]) -> Result<(), String> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        png(data)
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        gif(data)
    } else if data.starts_with(b"\xff\xd8") {
        jpeg(data)
    } else if data.starts_with(b"RIFF") {
        webp(data)
    } else {
        Err(INVALID.into())
    }
}
fn png(data: &[u8]) -> Result<(), String> {
    let mut input = Bytes::new(data);
    input.take(8)?;
    let mut canvas = None;
    let mut frames = 0;
    let mut declared = None;
    loop {
        let length = be32(input.take(4)?) as usize;
        let kind = input.take(4)?;
        let payload = input.take(length)?;
        input.take(4)?; // CRC is checked by the image decoder.
        if canvas.is_none() && kind != b"IHDR" {
            return Err(INVALID.into());
        }
        match kind {
            b"IHDR" if length == 13 && canvas.is_none() => {
                let size = (be32(&payload[..4]), be32(&payload[4..8]));
                check(size, 1)?;
                canvas = Some(size);
            }
            b"IHDR" => return Err(INVALID.into()),
            b"acTL" if length == 8 && declared.is_none() => {
                let count = be32(&payload[..4]);
                if count == 0 {
                    return Err(INVALID.into());
                }
                check(canvas.ok_or(INVALID)?, count)?;
                declared = Some(count);
            }
            b"fcTL" if length == 26 => {
                let size = (be32(&payload[4..8]), be32(&payload[8..12]));
                rectangle(
                    canvas.ok_or(INVALID)?,
                    be32(&payload[12..16]),
                    be32(&payload[16..20]),
                    size,
                )?;
                frames += 1;
                check(canvas.ok_or(INVALID)?, frames)?;
            }
            b"acTL" | b"fcTL" => return Err(INVALID.into()),
            b"IEND" if length == 0 => {
                if declared.is_some_and(|count| count != frames)
                    || (frames > 0 && declared.is_none())
                {
                    return Err(INVALID.into());
                }
                return Ok(());
            }
            b"IEND" => return Err(INVALID.into()),
            _ => {}
        }
    }
}
fn blocks(input: &mut Bytes<'_>) -> Result<(), String> {
    loop {
        let count = input.byte()? as usize;
        if count == 0 {
            return Ok(());
        }
        input.take(count)?;
    }
}
fn gif(data: &[u8]) -> Result<(), String> {
    let mut input = Bytes::new(data);
    input.take(6)?;
    let screen = input.take(7)?;
    let canvas = (le16(&screen[..2]), le16(&screen[2..4]));
    check(canvas, 1)?;
    if screen[4] & 0x80 != 0 {
        input.take(3usize << ((screen[4] & 7) + 1))?;
    }
    let mut frames = 0;
    loop {
        match input.byte()? {
            0x21 => {
                input.byte()?;
                blocks(&mut input)?;
            }
            0x2c => {
                let frame = input.take(9)?;
                rectangle(
                    canvas,
                    le16(&frame[..2]),
                    le16(&frame[2..4]),
                    (le16(&frame[4..6]), le16(&frame[6..8])),
                )?;
                frames += 1;
                check(canvas, frames)?;
                if frame[8] & 0x80 != 0 {
                    input.take(3usize << ((frame[8] & 7) + 1))?;
                }
                input.byte()?;
                blocks(&mut input)?;
            }
            0x3b if frames > 0 => return Ok(()),
            _ => return Err(INVALID.into()),
        }
    }
}
fn jpeg(data: &[u8]) -> Result<(), String> {
    let mut input = Bytes::new(data);
    input.take(2)?;
    let mut dimensions = None;
    loop {
        if input.at > 1024 * 1024 {
            return Err("JPEG artwork header exceeds 1 MiB".into());
        }
        if input.byte()? != 0xff {
            return Err(INVALID.into());
        }
        let mut marker = input.byte()?;
        while marker == 0xff {
            marker = input.byte()?;
        }
        if marker == 0xda {
            return dimensions.map(|_| ()).ok_or_else(|| INVALID.into());
        }
        if marker == 0xd9 || marker == 0xd8 || marker == 0 {
            return Err(INVALID.into());
        }
        if marker == 1 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let length = be16(input.take(2)?) as usize;
        let payload = input.take(length.checked_sub(2).ok_or(INVALID)?)?;
        if matches!(marker,0xc0..=0xc3|0xc5..=0xc7|0xc9..=0xcb|0xcd..=0xcf) {
            if payload.len() < 6 || dimensions.is_some() {
                return Err(INVALID.into());
            }
            let size = (be16(&payload[3..5]), be16(&payload[1..3]));
            check(size, 1)?;
            dimensions = Some(size);
        }
    }
}
fn raster(kind: &[u8], payload: &[u8]) -> Result<Size, String> {
    let size = match kind {
        b"VP8 " if payload.len() >= 10 && &payload[3..6] == b"\x9d\x01\x2a" => (
            le16(&payload[6..8]) & 0x3fff,
            le16(&payload[8..10]) & 0x3fff,
        ),
        b"VP8L" if payload.len() >= 5 && payload[0] == 0x2f => {
            let bits = le32(&payload[1..5]);
            ((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1)
        }
        _ => return Err(INVALID.into()),
    };
    check(size, 1)?;
    Ok(size)
}
fn chunk<'a>(input: &mut Bytes<'a>) -> Result<(&'a [u8], &'a [u8]), String> {
    let kind = input.take(4)?;
    let size = le32(input.take(4)?) as usize;
    let payload = input.take(size)?;
    if size % 2 == 1 {
        input.take(1)?;
    }
    Ok((kind, payload))
}
fn webp(data: &[u8]) -> Result<(), String> {
    let mut header = Bytes::new(data);
    header.take(4)?;
    let length = le32(header.take(4)?) as usize;
    if header.take(4)? != b"WEBP" || length < 4 {
        return Err(INVALID.into());
    }
    let mut input = Bytes::new(
        data.get(12..length.checked_add(8).ok_or(INVALID)?)
            .ok_or(INVALID)?,
    );
    let mut canvas = None;
    let mut animated = false;
    let mut frames = 0;
    let mut still = false;
    while input.at < input.data.len() {
        let (kind, payload) = chunk(&mut input)?;
        match kind {
            b"VP8X" if payload.len() == 10 && canvas.is_none() => {
                let size = (le24(&payload[4..7]) + 1, le24(&payload[7..10]) + 1);
                check(size, 1)?;
                canvas = Some(size);
                animated = payload[0] & 2 != 0;
            }
            b"VP8X" => return Err(INVALID.into()),
            b"VP8 " | b"VP8L" if !animated && !still => {
                let size = raster(kind, payload)?;
                if canvas.is_some_and(|canvas| canvas != size) {
                    return Err(INVALID.into());
                }
                canvas = Some(size);
                still = true;
            }
            b"ANMF" if animated && payload.len() >= 16 => {
                let size = (le24(&payload[6..9]) + 1, le24(&payload[9..12]) + 1);
                rectangle(
                    canvas.ok_or(INVALID)?,
                    le24(&payload[..3]) * 2,
                    le24(&payload[3..6]) * 2,
                    size,
                )?;
                frames += 1;
                check(canvas.ok_or(INVALID)?, frames)?;
                let mut nested = Bytes::new(&payload[16..]);
                let mut found = false;
                while nested.at < nested.data.len() {
                    let (kind, data) = chunk(&mut nested)?;
                    if kind == b"VP8 " || kind == b"VP8L" {
                        if found || raster(kind, data)? != size {
                            return Err(INVALID.into());
                        }
                        found = true;
                    } else if kind == b"ANMF" || kind == b"VP8X" {
                        return Err(INVALID.into());
                    }
                }
                if !found {
                    return Err(INVALID.into());
                }
            }
            b"ANMF" | b"VP8 " | b"VP8L" => return Err(INVALID.into()),
            _ => {}
        }
    }
    if (animated && frames > 0) || (!animated && still) {
        Ok(())
    } else {
        Err(INVALID.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn png_chunk(out: &mut Vec<u8>, kind: &[u8], payload: &[u8]) {
        out.extend((payload.len() as u32).to_be_bytes());
        out.extend(kind);
        out.extend(payload);
        out.extend([0; 4]);
    }
    fn png_header(width: u32, height: u32, frames: u32) -> Vec<u8> {
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut header = vec![0; 13];
        header[..4].copy_from_slice(&width.to_be_bytes());
        header[4..8].copy_from_slice(&height.to_be_bytes());
        png_chunk(&mut out, b"IHDR", &header);
        if frames > 1 {
            let mut control = vec![0; 8];
            control[..4].copy_from_slice(&frames.to_be_bytes());
            png_chunk(&mut out, b"acTL", &control);
            for _ in 0..frames {
                let mut frame = vec![0; 26];
                frame[4..8].copy_from_slice(&width.to_be_bytes());
                frame[8..12].copy_from_slice(&height.to_be_bytes());
                png_chunk(&mut out, b"fcTL", &frame);
            }
        }
        png_chunk(&mut out, b"IEND", &[]);
        out
    }
    fn gif_header(width: u16, height: u16, frames: u32) -> Vec<u8> {
        let mut out = b"GIF89a".to_vec();
        out.extend(width.to_le_bytes());
        out.extend(height.to_le_bytes());
        out.extend([0; 3]);
        for _ in 0..frames {
            out.push(0x2c);
            out.extend([0; 4]);
            out.extend(width.to_le_bytes());
            out.extend(height.to_le_bytes());
            out.extend([0, 2, 0]);
        }
        out.push(0x3b);
        out
    }
    fn jpeg_header(width: u16, height: u16) -> Vec<u8> {
        let mut out = vec![0xff, 0xd8, 0xff, 0xc0, 0, 8, 8];
        out.extend(height.to_be_bytes());
        out.extend(width.to_be_bytes());
        out.extend([1, 0xff, 0xda]);
        out
    }
    fn webp_chunk(out: &mut Vec<u8>, kind: &[u8], payload: &[u8]) {
        out.extend(kind);
        out.extend((payload.len() as u32).to_le_bytes());
        out.extend(payload);
        if payload.len() % 2 == 1 {
            out.push(0);
        }
    }
    fn webp_header(width: u32, height: u32, frames: u32) -> Vec<u8> {
        let mut raster = vec![0x2f];
        raster.extend(((width - 1) | ((height - 1) << 14)).to_le_bytes());
        let mut chunks = Vec::new();
        if frames == 1 {
            webp_chunk(&mut chunks, b"VP8L", &raster);
        } else {
            let mut extended = vec![0; 10];
            extended[0] = 2;
            extended[4..7].copy_from_slice(&(width - 1).to_le_bytes()[..3]);
            extended[7..10].copy_from_slice(&(height - 1).to_le_bytes()[..3]);
            webp_chunk(&mut chunks, b"VP8X", &extended);
            for _ in 0..frames {
                let mut frame = vec![0; 16];
                frame[6..9].copy_from_slice(&(width - 1).to_le_bytes()[..3]);
                frame[9..12].copy_from_slice(&(height - 1).to_le_bytes()[..3]);
                webp_chunk(&mut frame, b"VP8L", &raster);
                webp_chunk(&mut chunks, b"ANMF", &frame);
            }
        }
        let mut out = b"RIFF".to_vec();
        out.extend((chunks.len() as u32 + 4).to_le_bytes());
        out.extend(b"WEBP");
        out.extend(chunks);
        out
    }
    #[test]
    fn inspects_supported_headers_and_rejects_large_canvases_before_decoding() {
        for data in [
            png_header(32, 24, 1),
            gif_header(32, 24, 1),
            jpeg_header(32, 24),
            webp_header(32, 24, 1),
            webp_header(32, 24, 3),
            gif_header(32, 24, 3),
            png_header(32, 24, 3),
        ] {
            inspect(&data).unwrap();
        }
        for data in [
            png_header(8193, 1, 1),
            png_header(8192, 8192, 1),
            gif_header(8193, 1, 1),
            jpeg_header(8193, 1),
            webp_header(8193, 1, 1),
        ] {
            assert!(inspect(&data).unwrap_err().contains("pixels"));
        }
        for data in [
            png_header(4096, 4096, 5),
            gif_header(1024, 1024, 65),
            webp_header(1024, 1024, 65),
            gif_header(1, 1, 257),
        ] {
            assert!(inspect(&data).is_err());
        }
    }
    #[test]
    fn truncated_and_malformed_headers_fail_without_panicking() {
        for data in [
            png_header(1, 1, 1),
            gif_header(1, 1, 1),
            jpeg_header(1, 1),
            webp_header(1, 1, 1),
            webp_header(1, 1, 2),
        ] {
            for end in 0..data.len() {
                assert!(
                    inspect(&data[..end]).is_err(),
                    "accepted truncated header at {end}/{}",
                    data.len()
                );
            }
            for index in 0..data.len() {
                let mut damaged = data.clone();
                damaged[index] = 255;
                let _ = inspect(&damaged);
            }
        }
        assert!(inspect(b"<svg xmlns='http://www.w3.org/2000/svg'/>").is_err());
    }
    #[test]
    fn checks_file_size_and_real_png_artwork() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("art.png");
        std::fs::write(&path, include_bytes!("../../../assets/icons/radio.png")).unwrap();
        validate(&path).unwrap();
        let file = File::options().write(true).open(&path).unwrap();
        file.set_len(MAX_BYTES + 1).unwrap();
        assert!(validate(&path).unwrap_err().contains("25 MiB"));
    }
}
