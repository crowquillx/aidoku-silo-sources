//! A minimal, allocation-only ZIP reader for comic archives.
//!
//! Silo serves a manga chapter as a whole CBZ archive; Aidoku cannot stream a
//! remote archive, so the source downloads it and extracts the page images
//! here. Only the features a CBZ actually needs are implemented: stored and
//! deflated entries, read from the central directory.

use aidoku::{
    Result,
    alloc::{string::String, vec::Vec},
    prelude::*,
};
use core::cmp::Ordering;

const EOCD_SIG: u32 = 0x0605_4b50;
const CEN_SIG: u32 = 0x0201_4b50;
const LOC_SIG: u32 = 0x0403_4b50;

const IMAGE_EXTENSIONS: [&str; 7] = [".jpg", ".jpeg", ".png", ".webp", ".gif", ".avif", ".bmp"];

pub struct Entry {
    pub name: String,
    pub method: u16,
    pub flags: u16,
    pub comp_size: u64,
    pub uncomp_size: u64,
    pub local_offset: u64,
}

fn u16_at(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes([
        *data.get(offset)?,
        *data.get(offset + 1)?,
    ]))
}

fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *data.get(offset)?,
        *data.get(offset + 1)?,
        *data.get(offset + 2)?,
        *data.get(offset + 3)?,
    ]))
}

/// Locates the end-of-central-directory record by scanning backwards over the
/// (possibly empty) archive comment.
fn find_eocd(data: &[u8]) -> Result<usize> {
    if data.len() < 22 {
        bail!("Comic archive is truncated.");
    }
    let min = data.len().saturating_sub(22 + 0xffff);
    let mut pos = data.len() - 22;
    loop {
        if u32_at(data, pos) == Some(EOCD_SIG) {
            return Ok(pos);
        }
        if pos == min {
            break;
        }
        pos -= 1;
    }
    bail!("Comic archive is not a valid ZIP file.");
}

/// Reads the central directory and returns its file entries.
pub fn entries(data: &[u8]) -> Result<Vec<Entry>> {
    let eocd = find_eocd(data)?;
    let count = u16_at(data, eocd + 10).ok_or_else(|| error!("Truncated ZIP directory."))?;
    let cd_offset = u32_at(data, eocd + 16).ok_or_else(|| error!("Truncated ZIP directory."))?;
    let cd_size = u32_at(data, eocd + 12).ok_or_else(|| error!("Truncated ZIP directory."))?;

    if count == 0xffff || cd_offset == 0xffff_ffff || cd_size == 0xffff_ffff {
        bail!("ZIP64 comic archives are not supported.");
    }

    let mut result = Vec::with_capacity(count as usize);
    let mut pos = cd_offset as usize;
    for _ in 0..count {
        if u32_at(data, pos) != Some(CEN_SIG) {
            bail!("Corrupt ZIP central directory.");
        }
        let flags = u16_at(data, pos + 8).ok_or_else(|| error!("Truncated entry."))?;
        let method = u16_at(data, pos + 10).ok_or_else(|| error!("Truncated entry."))?;
        let comp_size = u32_at(data, pos + 20).ok_or_else(|| error!("Truncated entry."))? as u64;
        let uncomp_size = u32_at(data, pos + 24).ok_or_else(|| error!("Truncated entry."))? as u64;
        let name_len = u16_at(data, pos + 28).ok_or_else(|| error!("Truncated entry."))? as usize;
        let extra_len = u16_at(data, pos + 30).ok_or_else(|| error!("Truncated entry."))? as usize;
        let comment_len =
            u16_at(data, pos + 32).ok_or_else(|| error!("Truncated entry."))? as usize;
        let local_offset = u32_at(data, pos + 42).ok_or_else(|| error!("Truncated entry."))? as u64;
        let name_start = pos + 46;
        let name_bytes = data
            .get(name_start..name_start + name_len)
            .ok_or_else(|| error!("Truncated entry name."))?;
        let name = String::from_utf8_lossy(name_bytes).into_owned();
        result.push(Entry {
            name,
            method,
            flags,
            comp_size,
            uncomp_size,
            local_offset,
        });
        pos = name_start + name_len + extra_len + comment_len;
    }
    Ok(result)
}

/// Decompresses a single entry.
pub fn extract(data: &[u8], entry: &Entry) -> Result<Vec<u8>> {
    if entry.flags & 0x1 != 0 {
        bail!("Encrypted comic archives are not supported.");
    }
    let local = entry.local_offset as usize;
    if u32_at(data, local) != Some(LOC_SIG) {
        bail!("Corrupt ZIP entry header.");
    }
    let name_len = u16_at(data, local + 26).ok_or_else(|| error!("Truncated entry."))? as usize;
    let extra_len = u16_at(data, local + 28).ok_or_else(|| error!("Truncated entry."))? as usize;
    let start = local + 30 + name_len + extra_len;
    let end = start + entry.comp_size as usize;
    let compressed = data
        .get(start..end)
        .ok_or_else(|| error!("ZIP entry extends past the end of the archive."))?;

    match entry.method {
        0 => Ok(compressed.to_vec()),
        8 => {
            let limit = if entry.uncomp_size > 0 {
                entry.uncomp_size as usize
            } else {
                usize::MAX
            };
            miniz_oxide::inflate::decompress_to_vec_with_limit(compressed, limit)
                .map_err(|e| error!("Failed to decompress a page: {e:?}"))
        }
        other => {
            bail!("Unsupported ZIP compression method {other}; only CBZ archives are supported.")
        }
    }
}

/// Whether an archive path is an image page rather than metadata.
fn is_image(name: &str) -> bool {
    if name.ends_with('/') {
        return false;
    }
    let lower = name.to_ascii_lowercase();
    if lower.contains("__macosx") || lower.contains("/._") || lower.starts_with("._") {
        return false;
    }
    let base = lower.rsplit('/').next().unwrap_or("");
    if base.is_empty() || base.starts_with('.') {
        return false;
    }
    IMAGE_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}

/// Compares two paths the way a comic reader orders pages, treating runs of
/// digits as numbers (`page2` before `page10`).
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ac), Some(bc)) => {
                if ac.is_ascii_digit() && bc.is_ascii_digit() {
                    let mut an = 0u64;
                    while let Some(c) = ai.peek().copied() {
                        if !c.is_ascii_digit() {
                            break;
                        }
                        an = an
                            .saturating_mul(10)
                            .saturating_add((c as u8 - b'0') as u64);
                        ai.next();
                    }
                    let mut bn = 0u64;
                    while let Some(c) = bi.peek().copied() {
                        if !c.is_ascii_digit() {
                            break;
                        }
                        bn = bn
                            .saturating_mul(10)
                            .saturating_add((c as u8 - b'0') as u64);
                        bi.next();
                    }
                    match an.cmp(&bn) {
                        Ordering::Equal => continue,
                        other => return other,
                    }
                } else {
                    match ac.to_ascii_lowercase().cmp(&bc.to_ascii_lowercase()) {
                        Ordering::Equal => {
                            ai.next();
                            bi.next();
                        }
                        other => return other,
                    }
                }
            }
        }
    }
}

/// Extracts every image page from a comic archive in reading order.
pub fn comic_pages(data: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut files: Vec<Entry> = entries(data)?
        .into_iter()
        .filter(|entry| is_image(&entry.name))
        .collect();
    if files.is_empty() {
        bail!("The comic archive contains no images.");
    }
    files.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    let mut pages = Vec::with_capacity(files.len());
    for file in &files {
        pages.push(extract(data, file)?);
    }
    Ok(pages)
}
