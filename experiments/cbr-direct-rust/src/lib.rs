#![no_std]

extern crate alloc;

#[cfg(feature = "rars")]
pub fn rars_read_member_at(
	input: &[u8],
	index: usize,
) -> rars::Result<Option<alloc::vec::Vec<u8>>> {
	let archive = rars::ArchiveReader::read(input)?;
	archive.read_member_at(index, None)
}

#[cfg(feature = "rars-format")]
pub fn rars_format_detect(input: &[u8]) -> Option<rars_format::ArchiveSignature> {
	rars_format::detect_archive_family(input)
}

#[cfg(feature = "unrar-rs")]
pub fn unrar_rs_cursor_api_is_available(input: &[u8]) {
	let _ = unrar_rs::RarArchive::open(std::io::Cursor::new(input.to_vec()));
}

#[cfg(feature = "unrar-rs")]
extern crate std;

#[cfg(feature = "unrar")]
pub fn unrar_api_is_available() {
	let _ = unrar::Archive::new("archive.rar");
}

#[cfg(feature = "compcol-rar3")]
pub fn compcol_rar3_decoder_api(input: &[u8], output: &mut [u8], unpack_size: u64) {
	use compcol::Decoder as _;
	let mut decoder = compcol::rar3::Decoder::with_unpack_size(unpack_size);
	let _ = decoder.decode(input, output);
}

#[cfg(feature = "compcol-rar5")]
pub fn compcol_rar5_decoder_api(input: &[u8], output: &mut [u8], unpack_size: u64) {
	use compcol::Decoder as _;
	let mut decoder = compcol::rar5::Decoder::with_unpack_size(unpack_size);
	let _ = decoder.decode(input, output);
}

#[cfg(test)]
extern crate std;

#[cfg(all(test, feature = "rars"))]
mod rars_tests {
	use super::rars_read_member_at;

	#[test]
	fn reads_an_indexed_member_from_an_in_memory_rar() {
		let mut builder = rars::Builder::new(rars::ArchiveVersion::Rar50).store(true);
		builder
			.add_bytes(b"first.txt".to_vec(), b"first".to_vec(), None, None)
			.unwrap();
		builder
			.add_bytes(b"second.txt".to_vec(), b"second".to_vec(), None, None)
			.unwrap();
		let bytes = builder.to_bytes().unwrap();

		assert_eq!(
			rars_read_member_at(&bytes, 1).unwrap().unwrap(),
			b"second"
		);
		assert_eq!(rars_read_member_at(&bytes, 2).unwrap(), None);
	}
}
