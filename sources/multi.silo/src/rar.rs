//! Direct, allocation-only RAR decoding. Archive parsing lives in the
//! `silo-cbr-native` crate; this adapter owns Aidoku errors and page ordering.
use aidoku::{
	Result,
	alloc::{string::String, vec::Vec},
	prelude::*,
};

pub const MAX_ARCHIVE_BYTES: u64 = silo_cbr_native::MAX_ARCHIVE_BYTES as u64;

pub struct Page {
	pub index: usize,
	pub name: String,
}

/// The archive's image members in reading (natural filename) order.
pub fn list_pages(input: &[u8]) -> Result<Vec<Page>> {
	let members = silo_cbr_native::list_members(input)
		.map_err(|e| error!("RAR archive could not be read: {e}"))?;
	let mut pages: Vec<_> = members
		.into_iter()
		.filter(|m| !m.directory && crate::zip::is_image(&m.name))
		.map(|m| Page {
			index: m.index,
			name: m.name,
		})
		.collect();
	if pages.is_empty() {
		bail!("The RAR comic archive contains no readable images.");
	}
	pages.sort_by(|a, b| crate::zip::natural_cmp(&a.name, &b.name));
	Ok(pages)
}

/// Decodes every image page in one pass over the archive (each solid group
/// once), converting each page as soon as it is decoded so only one page is
/// held at a time. Returns the converted pages in reading order.
pub fn decode_pages<T>(input: &[u8], mut convert: impl FnMut(&[u8]) -> T) -> Result<Vec<T>> {
	let pages = list_pages(input)?;
	let mut positions: Vec<Option<usize>> = Vec::new();
	for (position, page) in pages.iter().enumerate() {
		if positions.len() <= page.index {
			positions.resize(page.index + 1, None);
		}
		positions[page.index] = Some(position);
	}
	let mut decoded: Vec<Option<T>> = pages.iter().map(|_| None).collect();
	silo_cbr_native::for_each_member(input, |index, bytes| {
		if let Some(&Some(position)) = positions.get(index) {
			decoded[position] = Some(convert(bytes));
		}
	})
	.map_err(|e| error!("RAR page could not be decoded: {e}"))?;
	decoded
		.into_iter()
		.map(|page| page.ok_or_else(|| error!("A RAR page was missing from the archive.")))
		.collect()
}
