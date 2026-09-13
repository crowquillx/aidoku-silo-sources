//! Direct, allocation-only RAR decoding. Archive parsing lives in the separate
//! experimental crate; this adapter owns Aidoku errors and page ordering.
use aidoku::{
	Result,
	alloc::{string::String, vec::Vec},
	prelude::*,
};

pub const MAX_ARCHIVE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_PAGE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_MEMBERS: usize = 512;

pub struct Page {
	pub index: u32,
	pub name: String,
	pub size: u64,
}

pub fn list_pages(input: &[u8]) -> Result<Vec<Page>> {
	let members = silo_cbr_native::list_members(input)
		.map_err(|e| error!("RAR archive could not be read: {e}"))?;
	let mut pages: Vec<_> = members
		.into_iter()
		.filter(|m| !m.directory && is_image(&m.name))
		.map(|m| Page {
			index: m.index as u32,
			name: m.name,
			size: m.size,
		})
		.collect();
	if pages.is_empty() {
		bail!("The RAR comic archive contains no readable images.");
	}
	pages.sort_by(|a, b| crate::zip::natural_cmp(&a.name, &b.name));
	Ok(pages)
}

pub fn extract_member(input: &[u8], index: u32) -> Result<Vec<u8>> {
	silo_cbr_native::extract_member(input, index as usize)
		.map_err(|e| error!("RAR page could not be decoded: {e}"))
}

pub fn decode_page(
	code: u16,
	data: &[u8],
	index: u64,
	total: u64,
	expected_size: u64,
) -> Result<Vec<u8>> {
	if !matches!(code, 200 | 206) {
		bail!("RAR page request returned HTTP {code}.");
	}
	if total == 0 || total > MAX_ARCHIVE_BYTES || data.len() as u64 != total {
		bail!("RAR page response is not the complete archive.");
	}
	if index >= MAX_MEMBERS as u64 {
		bail!("RAR page index is outside the member limit.");
	}
	if expected_size > MAX_PAGE_BYTES {
		bail!("RAR page exceeds the decoder size limit.");
	}
	let output = extract_member(data, index as u32)?;
	if output.len() as u64 != expected_size {
		bail!("RAR page size did not match its declared size.");
	}
	Ok(output)
}

fn is_image(name: &str) -> bool {
	let lower = name.to_ascii_lowercase();
	let base = lower.rsplit('/').next().unwrap_or("");
	!base.is_empty()
		&& !base.starts_with('.')
		&& !lower.contains("__macosx")
		&& !lower.contains("/._")
		&& [".jpg", ".jpeg", ".png", ".webp", ".gif", ".avif", ".bmp"]
			.iter()
			.any(|extension| lower.ends_with(extension))
}
