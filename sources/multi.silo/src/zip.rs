//! A minimal, allocation-only ZIP reader for comic archives.
//!
//! Silo serves a manga chapter as a whole CBZ archive. Aidoku can't stream a
//! remote archive as page images, but the archive is a ZIP, so the source reads
//! only what it needs over HTTP range requests: the central directory first,
//! then each page's compressed bytes on demand. This module owns all the ZIP
//! parsing; the network side lives in `client` and the Aidoku glue in `lib`.

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

/// How many trailing bytes to read when looking for the end-of-central
/// directory record. Covers the maximum ZIP comment length plus the record.
pub const TAIL_BYTES: u64 = 65_536 + 22;

pub struct Entry {
	pub name: String,
	pub method: u16,
	pub flags: u16,
	pub comp_size: u64,
	pub uncomp_size: u64,
	pub local_offset: u64,
}

/// The central directory location, as absolute offsets into the archive.
pub struct Directory {
	pub count: u16,
	pub cd_offset: u64,
	pub cd_size: u64,
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

/// Locates the end-of-central-directory record within a trailing slice.
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

/// Reads the central directory record from a trailing slice of the archive.
pub fn read_directory(data: &[u8]) -> Result<Directory> {
	let eocd = find_eocd(data)?;
	let count = u16_at(data, eocd + 10).ok_or_else(|| error!("Truncated ZIP directory."))?;
	let cd_size = u32_at(data, eocd + 12).ok_or_else(|| error!("Truncated ZIP directory."))? as u64;
	let cd_offset =
		u32_at(data, eocd + 16).ok_or_else(|| error!("Truncated ZIP directory."))? as u64;

	if count == 0xffff || cd_offset == 0xffff_ffff || cd_size == 0xffff_ffff {
		bail!("ZIP64 comic archives are not supported.");
	}
	Ok(Directory {
		count,
		cd_offset,
		cd_size,
	})
}

/// Parses central directory entries from a slice of the archive. `base_offset`
/// is the absolute offset of `data`; all entry positions are absolute.
pub fn parse_entries(data: &[u8], base_offset: u64, directory: &Directory) -> Result<Vec<Entry>> {
	let mut result = Vec::with_capacity(directory.count as usize);
	let mut pos = directory.cd_offset;
	for _ in 0..directory.count {
		let index_u64 = pos
			.checked_sub(base_offset)
			.ok_or_else(|| error!("Central directory is outside the fetched range."))?;
		let index = index_u64 as usize;
		if u32_at(data, index) != Some(CEN_SIG) {
			bail!("Corrupt ZIP central directory.");
		}
		let flags = u16_at(data, index + 8).ok_or_else(|| error!("Truncated entry."))?;
		let method = u16_at(data, index + 10).ok_or_else(|| error!("Truncated entry."))?;
		let comp_size = u32_at(data, index + 20).ok_or_else(|| error!("Truncated entry."))? as u64;
		let uncomp_size =
			u32_at(data, index + 24).ok_or_else(|| error!("Truncated entry."))? as u64;
		let name_len = u16_at(data, index + 28).ok_or_else(|| error!("Truncated entry."))? as usize;
		let extra_len =
			u16_at(data, index + 30).ok_or_else(|| error!("Truncated entry."))? as usize;
		let comment_len =
			u16_at(data, index + 32).ok_or_else(|| error!("Truncated entry."))? as usize;
		let local_offset =
			u32_at(data, index + 42).ok_or_else(|| error!("Truncated entry."))? as u64;
		let name_start = index + 46;
		let name_bytes = data
			.get(name_start..name_start + name_len)
			.ok_or_else(|| error!("Truncated entry name."))?;
		result.push(Entry {
			name: String::from_utf8_lossy(name_bytes).into_owned(),
			method,
			flags,
			comp_size,
			uncomp_size,
			local_offset,
		});
		pos += (46 + name_len + extra_len + comment_len) as u64;
	}
	Ok(result)
}

fn decompress(slice: &[u8], method: u16, uncomp_size: u64) -> Result<Vec<u8>> {
	match method {
		0 => Ok(slice.to_vec()),
		8 => {
			let limit = if uncomp_size > 0 {
				uncomp_size as usize
			} else {
				usize::MAX
			};
			miniz_oxide::inflate::decompress_to_vec_with_limit(slice, limit)
				.map_err(|e| error!("Failed to decompress a page: {e:?}"))
		}
		other => {
			bail!("Unsupported ZIP compression method {other}; only CBZ archives are supported.")
		}
	}
}

fn extract_at(
	data: &[u8],
	local: usize,
	method: u16,
	comp_size: u64,
	uncomp_size: u64,
) -> Result<Vec<u8>> {
	if u32_at(data, local) != Some(LOC_SIG) {
		bail!("Corrupt ZIP entry header.");
	}
	let name_len = u16_at(data, local + 26).ok_or_else(|| error!("Truncated entry."))? as usize;
	let extra_len = u16_at(data, local + 28).ok_or_else(|| error!("Truncated entry."))? as usize;
	let start = local + 30 + name_len + extra_len;
	let end = start + comp_size as usize;
	let slice = data
		.get(start..end)
		.ok_or_else(|| error!("ZIP entry extends past the fetched data."))?;
	decompress(slice, method, uncomp_size)
}

/// Extracts an entry from the full archive in memory.
pub fn extract_entry(
	data: &[u8],
	local_offset: u64,
	method: u16,
	comp_size: u64,
	uncomp_size: u64,
) -> Result<Vec<u8>> {
	extract_at(data, local_offset as usize, method, comp_size, uncomp_size)
}

/// Extracts an entry from a buffer that begins at the entry's local header.
pub fn extract_local(
	data: &[u8],
	method: u16,
	comp_size: u64,
	uncomp_size: u64,
) -> Result<Vec<u8>> {
	extract_at(data, 0, method, comp_size, uncomp_size)
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

/// Filters a directory listing down to image pages in reading order.
pub fn image_pages(entries: Vec<Entry>) -> Vec<Entry> {
	let mut pages: Vec<Entry> = entries
		.into_iter()
		.filter(|entry| is_image(&entry.name) && entry.flags & 0x1 == 0)
		.collect();
	pages.sort_by(|a, b| natural_cmp(&a.name, &b.name));
	pages
}
