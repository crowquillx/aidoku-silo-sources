use rars::{Archive, ArchiveReader};

#[unsafe(no_mangle)]
pub extern "C" fn input_alloc(len: u32) -> u32 {
	Box::into_raw(vec![0u8; len as usize].into_boxed_slice()) as *mut u8 as u32
}

/// # Safety
/// `ptr` must come from `input_alloc(len)` and not have been consumed.
///
// One call per instance: transfers input ownership and retains output until
// the interpreter drops the whole instance. High 32 bits = length, low = ptr.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rar_call(ptr: u32, len: u32, index: i32) -> u64 {
	let input = unsafe { Vec::from_raw_parts(ptr as *mut u8, len as usize, len as usize) };
	let result = (|| -> Result<Vec<u8>, String> {
		if input.len() > 16 * 1024 * 1024 {
			return Err("archive exceeds 16 MiB limit".into());
		}
		let archive = ArchiveReader::read_owned(input).map_err(|e| e.to_string())?;
		reject_rar5_redirections(&archive)?;
		let mut count = 0usize;
		let mut total = 0u64;
		for member in archive.members() {
			count += 1;
			total = total
				.checked_add(member.meta.unpacked_size)
				.ok_or("unpacked size overflow")?;
			if count > 512
				|| total > 64 * 1024 * 1024
				|| member.meta.unpacked_size > 16 * 1024 * 1024
			{
				return Err("archive exceeds experimental decoder limits".into());
			}
			if member.meta.is_encrypted || member.meta.is_split_before || member.meta.is_split_after
			{
				return Err("encrypted or split RAR entries are unsupported".into());
			}
		}
		if index < 0 {
			let members: Vec<_> = archive.members().enumerate().map(|(i,m)| serde_json::json!({"index":i,"name":String::from_utf8_lossy(&m.meta.name),"size":m.meta.unpacked_size,"directory":m.meta.is_directory,"encrypted":m.meta.is_encrypted,"split":m.meta.is_split_before||m.meta.is_split_after})).collect();
			return serde_json::to_vec(&members).map_err(|e| e.to_string());
		}
		archive
			.read_member_at(index as usize, None)
			.map_err(|e| e.to_string())?
			.ok_or_else(|| "entry missing".into())
	})();
	let output = match result {
		Ok(v) => v,
		Err(e) => format!("ERROR: {e}").into_bytes(),
	}
	.into_boxed_slice();
	let size = output.len() as u64;
	let ptr = Box::into_raw(output) as *mut u8 as u32;
	(size << 32) | u64::from(ptr)
}

fn reject_rar5_redirections(archive: &Archive) -> Result<(), String> {
	if let Archive::Rar50Plus(rar5) = archive
		&& rar5.files().any(|file| file.redirection.is_some())
	{
		return Err("RAR5 redirection entries are unsupported".into());
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::reject_rar5_redirections;
	use rars::ArchiveReader;
	use std::vec::Vec;

	fn fixture() -> Vec<u8> {
		include_str!("../tests/fixtures/rar5-redirection.hex")
			.split_whitespace()
			.map(|byte| u8::from_str_radix(byte, 16).unwrap())
			.collect()
	}

	#[test]
	fn rejects_rar5_redirection_before_member_listing_or_extraction() {
		let archive = ArchiveReader::read(&fixture()).unwrap();
		assert_eq!(archive.members().count(), 2);
		assert!(archive
			.extract_to(None, |_| Ok(Box::new(Vec::new())))
			.is_ok());
		assert_eq!(
			reject_rar5_redirections(&archive).unwrap_err(),
			"RAR5 redirection entries are unsupported"
		);
	}
}
