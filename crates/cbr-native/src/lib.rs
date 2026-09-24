#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use compcol::{Decoder, Status};

pub const MAX_ARCHIVE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_MEMBER_UNPACKED_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_TOTAL_UNPACKED_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 512;
pub const MAX_DICTIONARY_BYTES: usize = 8 * 1024 * 1024;

const RAR4_SIGNATURE: &[u8; 7] = b"Rar!\x1a\x07\x00";
const RAR5_SIGNATURE: &[u8; 8] = b"Rar!\x1a\x07\x01\x00";

const RAR4_MARK_HEAD: u8 = 0x72;
const RAR4_MAIN_HEAD: u8 = 0x73;
const RAR4_FILE_HEAD: u8 = 0x74;
const RAR4_ENDARC_HEAD: u8 = 0x7b;
const RAR4_LONG_BLOCK: u16 = 0x8000;
const RAR4_MHD_SOLID: u16 = 0x0008;
const RAR4_MHD_COMMENT: u16 = 0x0002;
const RAR4_MHD_VOLUME: u16 = 0x0001;
const RAR4_MHD_PROTECT: u16 = 0x0040;
const RAR4_MHD_PASSWORD: u16 = 0x0080;
const RAR4_MHD_FIRSTVOLUME: u16 = 0x0100;
const RAR4_MHD_ENCRYPTVER: u16 = 0x0200;
const RAR4_MHD_NEWNUMBERING: u16 = 0x0010;

const RAR4_FHD_SPLIT_BEFORE: u16 = 0x0001;
const RAR4_FHD_SPLIT_AFTER: u16 = 0x0002;
const RAR4_FHD_PASSWORD: u16 = 0x0004;
const RAR4_FHD_COMMENT: u16 = 0x0008;
const RAR4_FHD_SOLID: u16 = 0x0010;
const RAR4_FHD_LARGE: u16 = 0x0100;
const RAR4_FHD_SALT: u16 = 0x0400;
const RAR4_FHD_DIRECTORY_MASK: u16 = 0x00e0;
const RAR4_FHD_ALLOWED: u16 = 0xbfff;

const RAR5_HEAD_MAIN: u64 = 1;
const RAR5_HEAD_FILE: u64 = 2;
const RAR5_HEAD_CRYPT: u64 = 4;
const RAR5_HEAD_END: u64 = 5;
const RAR5_HFL_EXTRA: u64 = 0x0001;
const RAR5_HFL_DATA: u64 = 0x0002;
const RAR5_HFL_SPLIT_BEFORE: u64 = 0x0008;
const RAR5_HFL_SPLIT_AFTER: u64 = 0x0010;
const RAR5_EFL_NEXT_VOLUME: u64 = 0x0001;
const RAR5_MHFL_VOLUME: u64 = 0x0001;
const RAR5_MHFL_VOLUME_NUMBER: u64 = 0x0002;
const RAR5_MHFL_SOLID: u64 = 0x0004;
const RAR5_MHFL_RECOVERY: u64 = 0x0008;
const RAR5_MHFL_LOCKED: u64 = 0x0010;
const RAR5_FHFL_DIRECTORY: u64 = 0x0001;
const RAR5_FHFL_MTIME: u64 = 0x0002;
const RAR5_FHFL_CRC32: u64 = 0x0004;
const RAR5_EXTRA_CRYPT: u64 = 0x01;
const RAR5_EXTRA_REDIR: u64 = 0x05;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
	InvalidArchive(&'static str),
	Truncated(&'static str),
	ChecksumMismatch(&'static str),
	LimitExceeded(&'static str),
	Unsupported(&'static str),
	MemberIndex,
	Directory,
	Decoder(compcol::Error),
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::InvalidArchive(message) => write!(f, "invalid RAR archive: {message}"),
			Self::Truncated(message) => write!(f, "truncated RAR archive: {message}"),
			Self::ChecksumMismatch(message) => write!(f, "RAR checksum mismatch: {message}"),
			Self::LimitExceeded(message) => write!(f, "RAR limit exceeded: {message}"),
			Self::Unsupported(message) => write!(f, "unsupported RAR feature: {message}"),
			Self::MemberIndex => f.write_str("RAR member index is out of range"),
			Self::Directory => f.write_str("RAR member is a directory"),
			Self::Decoder(error) => write!(f, "RAR compressed data failed: {error}"),
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
	pub index: usize,
	pub name: String,
	pub size: u64,
	pub directory: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
	Rar4,
	Rar5,
}

#[derive(Debug, Clone)]
struct FileEntry {
	index: usize,
	name: Vec<u8>,
	size: u64,
	directory: bool,
	packed: core::ops::Range<usize>,
	crc32: Option<u32>,
	method: u8,
	solid: bool,
	dictionary: usize,
}

#[derive(Debug)]
struct Archive {
	family: Family,
	main_solid: bool,
	files: Vec<FileEntry>,
}

#[derive(Debug, Clone, Copy)]
struct Rar4Block {
	kind: u8,
	flags: u16,
	header_end: usize,
	total_end: usize,
}

#[derive(Debug, Clone, Copy)]
struct Rar5Block {
	kind: u64,
	flags: u64,
	type_start: usize,
	type_end: usize,
	extra_start: usize,
	extra_end: usize,
	data_start: usize,
	data_end: usize,
	next: usize,
}

/// Parse a bounded RAR container and return its members in archive order.
pub fn list_members(input: &[u8]) -> Result<Vec<Member>, Error> {
	let archive = parse_archive(input)?;
	Ok(archive
		.files
		.iter()
		.map(|file| Member {
			index: file.index,
			name: String::from_utf8_lossy(&file.name).into_owned(),
			size: file.size,
			directory: file.directory,
		})
		.collect())
}

/// Extract one member by archive index. A solid member decodes its group
/// from the start; use [`for_each_member`] to extract several.
pub fn extract_member(input: &[u8], index: usize) -> Result<Vec<u8>, Error> {
	let archive = parse_archive(input)?;
	let file = archive.files.get(index).ok_or(Error::MemberIndex)?;
	if file.directory {
		return Err(Error::Directory);
	}
	let start = solid_group_start(&archive, index);
	let mut selected = None;
	extract_group(input, &archive, start, index, &mut |member, output| {
		if member == index {
			selected = Some(output.to_vec());
		}
	})?;
	selected.ok_or(Error::InvalidArchive("RAR target member is missing"))
}

/// Extract every file member in archive order, decoding each solid group
/// once. `visit` receives each member's archive index and verified bytes.
pub fn for_each_member(input: &[u8], mut visit: impl FnMut(usize, &[u8])) -> Result<(), Error> {
	let archive = parse_archive(input)?;
	let mut start = 0usize;
	while start < archive.files.len() {
		let mut last = start;
		while last + 1 < archive.files.len() && solid_group_start(&archive, last + 1) <= start {
			last += 1;
		}
		extract_group(input, &archive, start, last, &mut visit)?;
		start = last + 1;
	}
	Ok(())
}

/// The first member of the solid group containing `index`.
fn solid_group_start(archive: &Archive, index: usize) -> usize {
	if archive.family == Family::Rar4 && archive.main_solid {
		return 0;
	}
	let mut start = index;
	while start > 0 && archive.files[start].solid {
		start -= 1;
	}
	start
}

/// Decodes members `start..=last`, which must begin a solid group (or be
/// independent members), and visits each file member's output.
fn extract_group(
	input: &[u8],
	archive: &Archive,
	start: usize,
	last: usize,
	visit: &mut dyn FnMut(usize, &[u8]),
) -> Result<(), Error> {
	let stored = |file: &FileEntry| match archive.family {
		Family::Rar4 => file.method == 0x30,
		Family::Rar5 => file.method == 0,
	};
	let group = &archive.files[start..=last];
	let solid = (archive.family == Family::Rar4 && archive.main_solid)
		|| group.iter().any(|file| file.solid);
	let compressed_group = solid && !group.iter().any(|file| file.directory || stored(file));
	if compressed_group {
		return match archive.family {
			Family::Rar4 => extract_rar4_solid(input, archive, start, last, visit),
			Family::Rar5 => extract_rar5_solid(input, archive, start, last, visit),
		};
	}
	for file in group.iter().filter(|file| !file.directory) {
		let output = match archive.family {
			Family::Rar4 => extract_rar4(input, file)?,
			Family::Rar5 => extract_rar5(input, file)?,
		};
		visit(file.index, &output);
	}
	Ok(())
}

fn parse_archive(input: &[u8]) -> Result<Archive, Error> {
	if input.is_empty() {
		return Err(Error::Truncated("archive is empty"));
	}
	if input.len() > MAX_ARCHIVE_BYTES {
		return Err(Error::LimitExceeded("archive is larger than 16 MiB"));
	}
	if input.starts_with(RAR4_SIGNATURE) {
		parse_rar4(input)
	} else if input.starts_with(RAR5_SIGNATURE) {
		parse_rar5(input)
	} else {
		Err(Error::Unsupported("RAR signature"))
	}
}

fn parse_rar4(input: &[u8]) -> Result<Archive, Error> {
	let marker = parse_rar4_block(input, 0)?;
	if marker.kind != RAR4_MARK_HEAD || marker.header_end != RAR4_SIGNATURE.len() {
		return Err(Error::InvalidArchive("RAR4 marker header"));
	}

	let main_pos = marker.header_end;
	let main = parse_rar4_block(input, main_pos)?;
	if main.kind != RAR4_MAIN_HEAD || main.header_end - main_pos < 13 {
		return Err(Error::InvalidArchive("RAR4 main header"));
	}
	let main_flags = main.flags;
	if main_flags
		& (RAR4_MHD_VOLUME
			| RAR4_MHD_COMMENT
			| RAR4_MHD_PROTECT
			| RAR4_MHD_PASSWORD
			| RAR4_MHD_FIRSTVOLUME
			| RAR4_MHD_ENCRYPTVER
			| RAR4_MHD_NEWNUMBERING)
		!= 0
	{
		return Err(Error::Unsupported(
			"RAR4 volume, recovery, comment, or encrypted headers",
		));
	}
	if main_flags & !RAR4_MHD_SOLID != 0 {
		return Err(Error::Unsupported("unknown RAR4 main flags"));
	}

	let mut files = Vec::new();
	let mut total_unpacked = 0u64;
	let mut pos = main.total_end;
	let mut saw_end = false;
	while pos < input.len() {
		let block = parse_rar4_block(input, pos)?;
		match block.kind {
			RAR4_FILE_HEAD => {
				if files.len() == MAX_ENTRIES {
					return Err(Error::LimitExceeded("archive has more than 512 entries"));
				}
				let file = parse_rar4_file(input, pos, block)?;
				total_unpacked = checked_total(total_unpacked, file.size)?;
				files.push(FileEntry {
					index: files.len(),
					name: file.name,
					size: file.size,
					directory: file.directory,
					packed: file.packed,
					crc32: Some(file.crc32),
					method: file.method,
					solid: file.solid,
					dictionary: 0,
				});
				pos = block.total_end;
			}
			RAR4_ENDARC_HEAD => {
				if block.flags != 0 {
					return Err(Error::Unsupported("RAR4 end header flags"));
				}
				saw_end = true;
				pos = block.total_end;
				break;
			}
			_ => return Err(Error::Unsupported("RAR4 block type")),
		}
	}
	// RAR4 permits EOF at a block boundary without an end header. The real
	// rars-generated fixtures omit it and independently extract with UnRAR.
	if !saw_end && pos != input.len() {
		return Err(Error::Truncated("RAR4 end header is missing"));
	}
	if pos != input.len() {
		return Err(Error::InvalidArchive("bytes follow the RAR4 end header"));
	}
	validate_solid_groups(
		&files,
		main_flags & RAR4_MHD_SOLID != 0,
		0x30,
		"mixed stored and compressed RAR4 solid group",
	)?;

	Ok(Archive {
		family: Family::Rar4,
		main_solid: main_flags & RAR4_MHD_SOLID != 0,
		files,
	})
}

struct Rar4File {
	name: Vec<u8>,
	size: u64,
	directory: bool,
	packed: core::ops::Range<usize>,
	crc32: u32,
	method: u8,
	solid: bool,
}

fn parse_rar4_file(input: &[u8], start: usize, block: Rar4Block) -> Result<Rar4File, Error> {
	if block.header_end - start < 32 {
		return Err(Error::InvalidArchive("RAR4 file header is too short"));
	}
	if block.flags & RAR4_LONG_BLOCK == 0 {
		return Err(Error::InvalidArchive("RAR4 file header has no data size"));
	}
	if block.flags & !RAR4_FHD_ALLOWED != 0 {
		return Err(Error::Unsupported("unknown RAR4 file flags"));
	}
	if block.flags
		& (RAR4_FHD_SPLIT_BEFORE
			| RAR4_FHD_SPLIT_AFTER
			| RAR4_FHD_PASSWORD
			| RAR4_FHD_COMMENT
			| RAR4_FHD_SALT)
		!= 0
	{
		return Err(Error::Unsupported(
			"RAR4 split, encrypted, or comment entry",
		));
	}

	let packed_low = read_u32(input, start + 7)? as u64;
	let unpacked_low = read_u32(input, start + 11)? as u64;
	let crc32 = read_u32(input, start + 16)?;
	let version = *input
		.get(start + 24)
		.ok_or(Error::Truncated("RAR4 file version"))?;
	let method = *input
		.get(start + 25)
		.ok_or(Error::Truncated("RAR4 file method"))?;
	let name_len = read_u16(input, start + 26)? as usize;
	let _attributes = read_u32(input, start + 28)?;
	let mut name_start = start + 32;
	let (packed, unpacked) = if block.flags & RAR4_FHD_LARGE != 0 {
		let high_packed = read_u32(input, name_start)? as u64;
		let high_unpacked = read_u32(input, name_start + 4)? as u64;
		name_start += 8;
		(
			(high_packed << 32) | packed_low,
			(high_unpacked << 32) | unpacked_low,
		)
	} else {
		(packed_low, unpacked_low)
	};
	if unpacked == u64::MAX {
		return Err(Error::Unsupported("unknown RAR4 unpacked size"));
	}
	check_member_size(unpacked)?;

	let name_end = name_start
		.checked_add(name_len)
		.ok_or(Error::InvalidArchive("RAR4 name length overflows"))?;
	if name_end > block.header_end {
		return Err(Error::Truncated("RAR4 name exceeds header"));
	}
	let name = input[name_start..name_end].to_vec();
	let directory = block.flags & RAR4_FHD_DIRECTORY_MASK == RAR4_FHD_DIRECTORY_MASK;
	let packed_len = usize::try_from(packed)
		.map_err(|_| Error::LimitExceeded("RAR4 packed member is too large"))?;
	let data_start = block.header_end;
	let data_end = data_start
		.checked_add(packed_len)
		.ok_or(Error::InvalidArchive("RAR4 data offset overflows"))?;
	if data_end != block.total_end {
		return Err(Error::InvalidArchive(
			"RAR4 data offset does not match header",
		));
	}
	if packed != unpacked {
		if method == 0x30 {
			return Err(Error::InvalidArchive("RAR4 stored sizes differ"));
		}
	} else if method == 0x30 {
		// Stored members are copied directly. The version byte is not a
		// compression version for this method.
	}
	if directory && (packed != 0 || unpacked != 0) {
		return Err(Error::InvalidArchive("RAR4 directory has data"));
	}
	if method != 0x30 {
		if !(0x31..=0x35).contains(&method) {
			return Err(Error::Unsupported("RAR4 compression method"));
		}
		if !(29..=36).contains(&version) {
			return Err(Error::Unsupported("RAR4 compression version"));
		}
	}

	Ok(Rar4File {
		name,
		size: unpacked,
		directory,
		packed: data_start..data_end,
		crc32,
		method,
		solid: block.flags & RAR4_FHD_SOLID != 0,
	})
}

fn parse_rar4_block(input: &[u8], start: usize) -> Result<Rar4Block, Error> {
	let minimum_end = start
		.checked_add(7)
		.ok_or(Error::InvalidArchive("RAR4 header offset overflows"))?;
	if minimum_end > input.len() {
		return Err(Error::Truncated("RAR4 block header"));
	}
	let expected_crc = read_u16(input, start)?;
	let kind = input[start + 2];
	let flags = read_u16(input, start + 3)?;
	let header_size = read_u16(input, start + 5)? as usize;
	if header_size < 7 {
		return Err(Error::InvalidArchive("RAR4 header size is below 7"));
	}
	let header_end = start
		.checked_add(header_size)
		.ok_or(Error::InvalidArchive("RAR4 header offset overflows"))?;
	if header_end > input.len() {
		return Err(Error::Truncated("RAR4 header body"));
	}
	let add_size = if flags & RAR4_LONG_BLOCK != 0 {
		usize::try_from(read_u32(input, start + 7)? as u64)
			.map_err(|_| Error::LimitExceeded("RAR4 block data is too large"))?
	} else {
		0
	};
	if flags & RAR4_LONG_BLOCK != 0 && header_size < 11 {
		return Err(Error::Truncated("RAR4 long block data size"));
	}
	let total_end = header_end
		.checked_add(add_size)
		.ok_or(Error::InvalidArchive("RAR4 block size overflows"))?;
	if total_end > input.len() {
		return Err(Error::Truncated("RAR4 block data"));
	}

	if kind != RAR4_MARK_HEAD {
		let crc_end = rar4_crc_end(input, start, kind, flags, header_end)?;
		let actual_crc = (crc32(&input[start + 2..crc_end]) & 0xffff) as u16;
		if actual_crc != expected_crc {
			return Err(Error::ChecksumMismatch("RAR4 header CRC32 low 16 bits"));
		}
	}
	Ok(Rar4Block {
		kind,
		flags,
		header_end,
		total_end,
	})
}

fn rar4_crc_end(
	input: &[u8],
	start: usize,
	kind: u8,
	flags: u16,
	header_end: usize,
) -> Result<usize, Error> {
	if kind == RAR4_MAIN_HEAD && flags & RAR4_MHD_COMMENT != 0 {
		return Ok((start + 13).min(header_end));
	}
	if kind == RAR4_FILE_HEAD && flags & RAR4_FHD_COMMENT != 0 {
		if header_end - start < 32 {
			return Err(Error::Truncated("RAR4 file header CRC range"));
		}
		let name_len = read_u16(input, start + 26)? as usize;
		let mut end = start + 32;
		if flags & RAR4_FHD_LARGE != 0 {
			end = end
				.checked_add(8)
				.ok_or(Error::InvalidArchive("RAR4 file CRC range overflows"))?;
		}
		end = end
			.checked_add(name_len)
			.ok_or(Error::InvalidArchive("RAR4 file CRC range overflows"))?;
		if flags & RAR4_FHD_SALT != 0 {
			end = end
				.checked_add(8)
				.ok_or(Error::InvalidArchive("RAR4 file CRC range overflows"))?;
		}
		return Ok(end.min(header_end));
	}
	Ok(header_end)
}

fn parse_rar5(input: &[u8]) -> Result<Archive, Error> {
	let main_pos = RAR5_SIGNATURE.len();
	let main = parse_rar5_block(input, main_pos)?;
	if main.kind != RAR5_HEAD_MAIN {
		return Err(Error::InvalidArchive("RAR5 main header is missing"));
	}
	let mut cursor = Cursor::new(input, main.type_start, main.type_end);
	let archive_flags = cursor.vint()?;
	if archive_flags
		& (RAR5_MHFL_VOLUME | RAR5_MHFL_VOLUME_NUMBER | RAR5_MHFL_RECOVERY | RAR5_MHFL_LOCKED)
		!= 0
	{
		return Err(Error::Unsupported(
			"RAR5 volume, recovery, or encrypted archive",
		));
	}
	if archive_flags & !RAR5_MHFL_SOLID != 0 {
		return Err(Error::Unsupported("unknown RAR5 main flags"));
	}
	if cursor.remaining() != 0 {
		return Err(Error::Unsupported("RAR5 main header fields"));
	}
	validate_rar5_extras(input, main.extra_start, main.extra_end)?;

	let mut files = Vec::new();
	let mut total_unpacked = 0u64;
	let mut pos = main.next;
	let mut saw_end = false;
	while pos < input.len() {
		let block = parse_rar5_block(input, pos)?;
		match block.kind {
			RAR5_HEAD_FILE => {
				if files.len() == MAX_ENTRIES {
					return Err(Error::LimitExceeded("archive has more than 512 entries"));
				}
				let file = parse_rar5_file(input, block)?;
				total_unpacked = checked_total(total_unpacked, file.size)?;
				files.push(file_with_index(file, files.len()));
				pos = block.next;
			}
			RAR5_HEAD_END => {
				let mut end_cursor = Cursor::new(input, block.type_start, block.type_end);
				let end_flags = if end_cursor.remaining() == 0 {
					0
				} else {
					end_cursor.vint()?
				};
				if end_flags & RAR5_EFL_NEXT_VOLUME != 0 || end_flags != 0 {
					return Err(Error::Unsupported("RAR5 volume end flags"));
				}
				if end_cursor.remaining() != 0 {
					return Err(Error::Unsupported("RAR5 end header fields"));
				}
				saw_end = true;
				pos = block.next;
				break;
			}
			RAR5_HEAD_CRYPT => return Err(Error::Unsupported("RAR5 encrypted headers")),
			_ => return Err(Error::Unsupported("RAR5 service or unknown block")),
		}
	}
	if !saw_end {
		return Err(Error::Truncated("RAR5 end header is missing"));
	}
	if pos != input.len() {
		return Err(Error::InvalidArchive("bytes follow the RAR5 end header"));
	}
	validate_solid_groups(
		&files,
		archive_flags & RAR5_MHFL_SOLID != 0,
		0,
		"mixed stored and compressed RAR5 solid group",
	)?;

	Ok(Archive {
		family: Family::Rar5,
		main_solid: archive_flags & RAR5_MHFL_SOLID != 0,
		files,
	})
}

fn file_with_index(mut file: FileEntry, index: usize) -> FileEntry {
	file.index = index;
	file
}

fn parse_rar5_file(input: &[u8], block: Rar5Block) -> Result<FileEntry, Error> {
	if block.flags & RAR5_HFL_SPLIT_BEFORE != 0 || block.flags & RAR5_HFL_SPLIT_AFTER != 0 {
		return Err(Error::Unsupported("RAR5 split entry"));
	}
	let mut cursor = Cursor::new(input, block.type_start, block.type_end);
	let file_flags = cursor.vint()?;
	if file_flags & !(RAR5_FHFL_DIRECTORY | RAR5_FHFL_MTIME | RAR5_FHFL_CRC32) != 0 {
		return Err(Error::Unsupported("unknown RAR5 file flags"));
	}
	let size = cursor.vint()?;
	if size == u64::MAX {
		return Err(Error::Unsupported("unknown RAR5 unpacked size"));
	}
	check_member_size(size)?;
	let _attributes = cursor.vint()?;
	if file_flags & RAR5_FHFL_MTIME != 0 {
		cursor.bytes(4)?;
	}
	let crc32 = if file_flags & RAR5_FHFL_CRC32 != 0 {
		Some(cursor.u32()?)
	} else {
		None
	};
	let compression_info = cursor.vint()?;
	let _host_os = cursor.vint()?;
	let name_len = usize::try_from(cursor.vint()?)
		.map_err(|_| Error::LimitExceeded("RAR5 name is too large"))?;
	let name = cursor.bytes(name_len)?.to_vec();
	if cursor.remaining() != 0 {
		return Err(Error::Unsupported("RAR5 file header fields"));
	}
	validate_rar5_extras(input, block.extra_start, block.extra_end)?;

	if block.flags & !(RAR5_HFL_EXTRA | RAR5_HFL_DATA) != 0 {
		return Err(Error::Unsupported("unknown RAR5 block flags"));
	}
	let directory = file_flags & RAR5_FHFL_DIRECTORY != 0;
	let data_len = block.data_end - block.data_start;
	if !directory && block.flags & RAR5_HFL_DATA == 0 {
		return Err(Error::InvalidArchive("RAR5 file has no data block"));
	}
	if directory && size != 0 {
		return Err(Error::InvalidArchive("RAR5 directory has unpacked data"));
	}

	let (method, solid, dictionary) = parse_rar5_compression_info(compression_info)?;
	if method == 0 {
		if data_len as u64 != size {
			return Err(Error::InvalidArchive("RAR5 stored sizes differ"));
		}
	} else if directory || block.flags & RAR5_HFL_DATA == 0 {
		return Err(Error::InvalidArchive("RAR5 compressed directory data"));
	}
	if directory && data_len != 0 {
		return Err(Error::InvalidArchive("RAR5 directory has packed data"));
	}

	Ok(FileEntry {
		index: 0,
		name,
		size,
		directory,
		packed: block.data_start..block.data_end,
		crc32,
		method: method as u8,
		solid,
		dictionary,
	})
}

fn parse_rar5_compression_info(info: u64) -> Result<(u64, bool, usize), Error> {
	if info & 0x3f != 0 {
		return Err(Error::Unsupported("RAR5 compression algorithm version"));
	}
	if info & !0x3fff != 0 {
		return Err(Error::Unsupported("RAR5 compression fields"));
	}
	let method = (info >> 7) & 0x07;
	if method > 5 {
		return Err(Error::Unsupported("RAR5 compression method"));
	}
	let shift = ((info >> 10) & 0x0f) as usize;
	let dictionary = 128usize
		.checked_mul(1024)
		.and_then(|value| value.checked_shl(shift as u32))
		.ok_or(Error::LimitExceeded("RAR5 dictionary size overflows"))?;
	if dictionary > MAX_DICTIONARY_BYTES {
		return Err(Error::LimitExceeded("RAR5 dictionary is larger than 8 MiB"));
	}
	Ok((method, info & 0x40 != 0, dictionary))
}

fn parse_rar5_block(input: &[u8], start: usize) -> Result<Rar5Block, Error> {
	if start
		.checked_add(5)
		.ok_or(Error::InvalidArchive("RAR5 header offset overflows"))?
		> input.len()
	{
		return Err(Error::Truncated("RAR5 block header"));
	}
	let expected_crc = read_u32(input, start)?;
	let (header_size, size_len) = read_vint(input, start + 4, input.len())?;
	let header_size = usize::try_from(header_size)
		.map_err(|_| Error::LimitExceeded("RAR5 header is too large"))?;
	let header_end = start
		.checked_add(4)
		.and_then(|value| value.checked_add(size_len))
		.and_then(|value| value.checked_add(header_size))
		.ok_or(Error::InvalidArchive("RAR5 header size overflows"))?;
	if header_end > input.len() {
		return Err(Error::Truncated("RAR5 header body"));
	}
	if crc32(&input[start + 4..header_end]) != expected_crc {
		return Err(Error::ChecksumMismatch("RAR5 header CRC32"));
	}

	let type_start = start + 4 + size_len;
	let mut cursor = Cursor::new(input, type_start, header_end);
	let kind = cursor.vint()?;
	let flags = cursor.vint()?;
	if flags & !(RAR5_HFL_EXTRA | RAR5_HFL_DATA | RAR5_HFL_SPLIT_BEFORE | RAR5_HFL_SPLIT_AFTER) != 0
	{
		return Err(Error::Unsupported("unknown RAR5 block flags"));
	}
	if flags & (RAR5_HFL_SPLIT_BEFORE | RAR5_HFL_SPLIT_AFTER) != 0 {
		return Err(Error::Unsupported("RAR5 split block"));
	}
	let extra_len = if flags & RAR5_HFL_EXTRA != 0 {
		usize::try_from(cursor.vint()?)
			.map_err(|_| Error::LimitExceeded("RAR5 extra area is too large"))?
	} else {
		0
	};
	let data_len = if flags & RAR5_HFL_DATA != 0 {
		usize::try_from(cursor.vint()?)
			.map_err(|_| Error::LimitExceeded("RAR5 data block is too large"))?
	} else {
		0
	};
	let type_end = header_end
		.checked_sub(extra_len)
		.ok_or(Error::InvalidArchive("RAR5 extra area exceeds header"))?;
	if cursor.pos > type_end {
		return Err(Error::Truncated("RAR5 type-specific header"));
	}
	let data_start = header_end;
	let data_end = data_start
		.checked_add(data_len)
		.ok_or(Error::InvalidArchive("RAR5 data offset overflows"))?;
	if data_end > input.len() {
		return Err(Error::Truncated("RAR5 data block"));
	}
	Ok(Rar5Block {
		kind,
		flags,
		type_start: cursor.pos,
		type_end,
		extra_start: type_end,
		extra_end: header_end,
		data_start,
		data_end,
		next: data_end,
	})
}

fn validate_rar5_extras(input: &[u8], start: usize, end: usize) -> Result<(), Error> {
	let mut pos = start;
	while pos < end {
		let (size, size_len) = read_vint(input, pos, end)?;
		let payload_start = pos
			.checked_add(size_len)
			.ok_or(Error::InvalidArchive("RAR5 extra offset overflows"))?;
		let payload_len = usize::try_from(size)
			.map_err(|_| Error::LimitExceeded("RAR5 extra record is too large"))?;
		if payload_len == 0 {
			return Err(Error::InvalidArchive("RAR5 extra record is empty"));
		}
		let record_end = payload_start
			.checked_add(payload_len)
			.ok_or(Error::InvalidArchive("RAR5 extra record overflows"))?;
		if record_end > end {
			return Err(Error::Truncated("RAR5 extra record"));
		}
		let (record_type, type_len) = read_vint(input, payload_start, record_end)?;
		if record_type == RAR5_EXTRA_CRYPT {
			return Err(Error::Unsupported("RAR5 encrypted entry"));
		}
		if record_type == RAR5_EXTRA_REDIR {
			return Err(Error::Unsupported("RAR5 redirection entry"));
		}
		if type_len == 0 {
			return Err(Error::InvalidArchive("RAR5 extra record type"));
		}
		pos = record_end;
	}
	Ok(())
}

fn validate_solid_groups(
	files: &[FileEntry],
	archive_solid: bool,
	stored_method: u8,
	message: &'static str,
) -> Result<(), Error> {
	if archive_solid {
		return validate_solid_range(files, 0, files.len(), stored_method, message);
	}

	let mut index = 0usize;
	while index < files.len() {
		if files[index].solid {
			let start = index.saturating_sub(1);
			let mut end = index + 1;
			while end < files.len() && files[end].solid {
				end += 1;
			}
			validate_solid_range(files, start, end, stored_method, message)?;
			index = end;
		} else {
			index += 1;
		}
	}
	Ok(())
}

fn validate_solid_range(
	files: &[FileEntry],
	start: usize,
	end: usize,
	stored_method: u8,
	message: &'static str,
) -> Result<(), Error> {
	let group = &files[start..end];
	let has_stored = group
		.iter()
		.any(|file| file.directory || file.method == stored_method);
	let has_compressed = group
		.iter()
		.any(|file| !file.directory && file.method != stored_method);
	if has_stored && has_compressed {
		return Err(Error::Unsupported(message));
	}
	Ok(())
}

/// Extracts a stored or independently compressed RAR4 member.
fn extract_rar4(input: &[u8], target: &FileEntry) -> Result<Vec<u8>, Error> {
	if target.method == 0x30 {
		let output = input[target.packed.clone()].to_vec();
		verify_crc(output.as_slice(), target.crc32)?;
		return Ok(output);
	}
	reject_initial_rar3_ppmd(input, target.packed.clone())?;
	let mut decoder = compcol::rar3::Decoder::with_unpack_size(target.size);
	let output = decode_one(&mut decoder, input, target.packed.clone(), target.size)?;
	verify_crc(&output, target.crc32)?;
	Ok(output)
}

/// Decodes a RAR4 solid group with one decoder, member by member.
fn extract_rar4_solid(
	input: &[u8],
	archive: &Archive,
	start: usize,
	last: usize,
	visit: &mut dyn FnMut(usize, &[u8]),
) -> Result<(), Error> {
	let first = &archive.files[start];
	reject_initial_rar3_ppmd(input, first.packed.clone())?;
	let mut decoder = compcol::rar3::Decoder::with_unpack_size(first.size).with_solid();
	for file in &archive.files[start..=last] {
		if file.index != start {
			decoder
				.begin_solid_member(file.size)
				.map_err(Error::Decoder)?;
		}
		let output = decode_one(&mut decoder, input, file.packed.clone(), file.size)?;
		verify_crc(&output, file.crc32)?;
		visit(file.index, &output);
	}
	Ok(())
}

/// Extracts a stored or independently compressed RAR5 member.
fn extract_rar5(input: &[u8], target: &FileEntry) -> Result<Vec<u8>, Error> {
	if target.method == 0 {
		let output = input[target.packed.clone()].to_vec();
		verify_optional_crc(&output, target.crc32)?;
		return Ok(output);
	}
	let window = target.dictionary;
	let mut decoder = compcol::rar5::Decoder::with_unpack_size_and_window(target.size, window);
	let output = decode_one(&mut decoder, input, target.packed.clone(), target.size)?;
	verify_optional_crc(&output, target.crc32)?;
	Ok(output)
}

/// Decodes a RAR5 solid group as one stream with file boundaries.
fn extract_rar5_solid(
	input: &[u8],
	archive: &Archive,
	start: usize,
	last: usize,
	visit: &mut dyn FnMut(usize, &[u8]),
) -> Result<(), Error> {
	let group = &archive.files[start..=last];
	if group.iter().any(|file| file.method == 0 || file.directory) {
		return Err(Error::Unsupported(
			"mixed stored and compressed RAR5 solid group",
		));
	}
	let mut dictionary = 0usize;
	let mut total_size = 0u64;
	let mut packed = Vec::new();
	let mut boundaries = Vec::with_capacity(group.len());
	for file in group {
		dictionary = dictionary.max(file.dictionary);
		total_size = checked_total(total_size, file.size)?;
		boundaries.push(total_size - file.size);
		packed.extend_from_slice(&input[file.packed.clone()]);
	}
	let mut decoder = compcol::rar5::Decoder::with_unpack_size_and_window(total_size, dictionary);
	for boundary in boundaries {
		decoder.add_file_boundary(boundary);
	}
	let output = decode_one(&mut decoder, &packed, 0..packed.len(), total_size)?;

	let mut offset = 0usize;
	for file in group {
		let end = offset
			.checked_add(
				usize::try_from(file.size)
					.map_err(|_| Error::LimitExceeded("member is too large"))?,
			)
			.ok_or(Error::LimitExceeded("solid output offset overflows"))?;
		verify_optional_crc(&output[offset..end], file.crc32)?;
		visit(file.index, &output[offset..end]);
		offset = end;
	}
	Ok(())
}

fn reject_initial_rar3_ppmd(input: &[u8], range: core::ops::Range<usize>) -> Result<(), Error> {
	let first = *input
		.get(range.start)
		.ok_or(Error::Truncated("RAR3 compressed data"))?;
	if first & 0x80 != 0 {
		return Err(Error::Unsupported(
			"RAR3 PPMd is rejected because compcol may allocate up to 256 MiB",
		));
	}
	Ok(())
}

fn decode_one<D: Decoder + ?Sized>(
	decoder: &mut D,
	input: &[u8],
	range: core::ops::Range<usize>,
	expected_size: u64,
) -> Result<Vec<u8>, Error> {
	let expected = usize::try_from(expected_size)
		.map_err(|_| Error::LimitExceeded("unpacked member is too large"))?;
	let mut output = vec![0u8; expected];
	let mut input_pos = range.start;
	let mut output_pos = 0usize;
	let mut steps = 0usize;
	loop {
		steps = steps.saturating_add(1);
		if steps > input.len().saturating_add(expected).saturating_add(1024) {
			return Err(Error::Decoder(compcol::Error::Corrupt));
		}
		let (progress, status) = if input_pos < range.end {
			decoder
				.decode(&input[input_pos..range.end], &mut output[output_pos..])
				.map_err(Error::Decoder)?
		} else {
			decoder
				.finish(&mut output[output_pos..])
				.map_err(Error::Decoder)?
		};
		input_pos = input_pos
			.checked_add(progress.consumed)
			.ok_or(Error::InvalidArchive("decoder input offset overflows"))?;
		output_pos = output_pos
			.checked_add(progress.written)
			.ok_or(Error::InvalidArchive("decoder output offset overflows"))?;
		if input_pos > range.end || output_pos > expected {
			return Err(Error::Decoder(compcol::Error::Corrupt));
		}
		if status == Status::StreamEnd {
			if output_pos != expected {
				return Err(Error::Decoder(compcol::Error::UnexpectedEnd));
			}
			return Ok(output);
		}
		if progress.consumed == 0 && progress.written == 0 {
			return Err(Error::Decoder(compcol::Error::UnexpectedEnd));
		}
	}
}

fn verify_crc(data: &[u8], expected: Option<u32>) -> Result<(), Error> {
	let expected = expected.ok_or(Error::InvalidArchive("missing member CRC"))?;
	if crc32(data) != expected {
		return Err(Error::ChecksumMismatch("member output CRC32"));
	}
	Ok(())
}

fn verify_optional_crc(data: &[u8], expected: Option<u32>) -> Result<(), Error> {
	if let Some(expected) = expected
		&& crc32(data) != expected
	{
		return Err(Error::ChecksumMismatch("member output CRC32"));
	}
	Ok(())
}

fn checked_total(total: u64, size: u64) -> Result<u64, Error> {
	let total = total
		.checked_add(size)
		.ok_or(Error::LimitExceeded("total unpacked size overflows"))?;
	if total > MAX_TOTAL_UNPACKED_BYTES {
		return Err(Error::LimitExceeded(
			"total unpacked data is larger than 64 MiB",
		));
	}
	Ok(total)
}

fn check_member_size(size: u64) -> Result<(), Error> {
	if size > MAX_MEMBER_UNPACKED_BYTES {
		return Err(Error::LimitExceeded("member is larger than 16 MiB"));
	}
	Ok(())
}

fn read_u16(input: &[u8], offset: usize) -> Result<u16, Error> {
	let bytes = input
		.get(offset..offset + 2)
		.ok_or(Error::Truncated("u16 field"))?;
	Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(input: &[u8], offset: usize) -> Result<u32, Error> {
	let bytes = input
		.get(offset..offset + 4)
		.ok_or(Error::Truncated("u32 field"))?;
	Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_vint(input: &[u8], offset: usize, end: usize) -> Result<(u64, usize), Error> {
	let mut value = 0u64;
	let mut shift = 0u32;
	for index in 0..10 {
		let pos = offset
			.checked_add(index)
			.ok_or(Error::InvalidArchive("RAR5 vint offset overflows"))?;
		if pos >= end || pos >= input.len() {
			return Err(Error::Truncated("RAR5 vint"));
		}
		let byte = input[pos];
		if shift == 63 && byte & 0x7e != 0 {
			return Err(Error::InvalidArchive("RAR5 vint overflows u64"));
		}
		value = value
			.checked_add(((byte & 0x7f) as u64) << shift)
			.ok_or(Error::InvalidArchive("RAR5 vint overflows u64"))?;
		if byte & 0x80 == 0 {
			return Ok((value, index + 1));
		}
		shift += 7;
	}
	Err(Error::InvalidArchive("RAR5 vint is too long"))
}

struct Cursor<'a> {
	input: &'a [u8],
	pos: usize,
	end: usize,
}

impl<'a> Cursor<'a> {
	fn new(input: &'a [u8], pos: usize, end: usize) -> Self {
		Self { input, pos, end }
	}

	fn vint(&mut self) -> Result<u64, Error> {
		let (value, len) = read_vint(self.input, self.pos, self.end)?;
		self.pos += len;
		Ok(value)
	}

	fn bytes(&mut self, length: usize) -> Result<&'a [u8], Error> {
		let end = self
			.pos
			.checked_add(length)
			.ok_or(Error::InvalidArchive("RAR5 field offset overflows"))?;
		if end > self.end {
			return Err(Error::Truncated("RAR5 type-specific field"));
		}
		let bytes = &self.input[self.pos..end];
		self.pos = end;
		Ok(bytes)
	}

	fn u32(&mut self) -> Result<u32, Error> {
		let bytes = self.bytes(4)?;
		Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
	}

	fn remaining(&self) -> usize {
		self.end.saturating_sub(self.pos)
	}
}

const CRC32_NIBBLE_TABLE: [u32; 16] = [
	0x0000_0000,
	0x1db7_1064,
	0x3b6e_20c8,
	0x26d9_30ac,
	0x76dc_4190,
	0x6b6b_51f4,
	0x4db2_6158,
	0x5005_713c,
	0xedb8_8320,
	0xf00f_9344,
	0xd6d6_a3e8,
	0xcb61_b38c,
	0x9b64_c2b0,
	0x86d3_d2d4,
	0xa00a_e278,
	0xbdbd_f21c,
];

fn crc32(bytes: &[u8]) -> u32 {
	let mut crc = !0u32;
	for &byte in bytes {
		crc = (crc >> 4) ^ CRC32_NIBBLE_TABLE[((crc ^ u32::from(byte)) & 0x0f) as usize];
		crc = (crc >> 4) ^ CRC32_NIBBLE_TABLE[((crc ^ u32::from(byte >> 4)) & 0x0f) as usize];
	}
	!crc
}

#[cfg(test)]
mod tests {
	extern crate std;

	use super::*;

	fn fixture(name: &str) -> &'static [u8] {
		match name {
			"rar40-normal" => include_bytes!("../fixtures/rar40-normal.cbr"),
			"rar40-solid" => include_bytes!("../fixtures/rar40-solid.cbr"),
			"rar50-normal" => include_bytes!("../fixtures/rar50-normal.cbr"),
			"rar50-solid" => include_bytes!("../fixtures/rar50-solid.cbr"),
			_ => panic!("unknown fixture {name}"),
		}
	}

	#[test]
	fn crc32_matches_standard_vector() {
		assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
	}

	#[test]
	fn lists_real_fixture_members_in_archive_order() {
		for name in ["rar40-normal", "rar40-solid", "rar50-normal", "rar50-solid"] {
			let members = list_members(fixture(name)).unwrap();
			assert_eq!(members.len(), 3, "{name}");
			assert_eq!(members[0].name, "page10.png", "{name}");
			assert_eq!(members[1].name, "page2.png", "{name}");
			assert_eq!(members[2].name, "page1.png", "{name}");
			assert!(members.iter().all(|member| member.size == 49_348));
			assert!(members.iter().all(|member| !member.directory));
		}
	}

	#[test]
	fn extracts_all_real_fixture_pages_exactly() {
		let expected = [
			include_bytes!("../fixtures/page10.png").as_slice(),
			include_bytes!("../fixtures/page2.png").as_slice(),
			include_bytes!("../fixtures/page1.png").as_slice(),
		];
		for name in ["rar40-normal", "rar40-solid", "rar50-normal", "rar50-solid"] {
			for (index, expected) in expected.iter().enumerate() {
				assert_eq!(
					extract_member(fixture(name), index).unwrap(),
					*expected,
					"{name} {index}"
				);
			}
		}
	}

	#[test]
	fn rejects_truncated_and_corrupt_headers() {
		let input = fixture("rar50-normal");
		assert!(matches!(
			list_members(&input[..input.len() - 1]),
			Err(Error::Truncated(_))
		));
		let mut corrupt = input.to_vec();
		corrupt[8] ^= 1;
		assert!(matches!(
			list_members(&corrupt),
			Err(Error::ChecksumMismatch(_))
		));

		let mut rar4_corrupt = fixture("rar40-normal").to_vec();
		rar4_corrupt[10] ^= 1;
		assert!(matches!(
			list_members(&rar4_corrupt),
			Err(Error::ChecksumMismatch(_))
		));
	}

	#[test]
	fn accepts_optional_rar4_end_header_but_rejects_partial_header() {
		for name in ["rar40-normal", "rar40-solid"] {
			let input = fixture(name);
			assert_eq!(list_members(input).unwrap().len(), 3);
			let mut end = [0, 0, RAR4_ENDARC_HEAD, 0, 0, 7, 0];
			let checksum = (crc32(&end[2..]) as u16).to_le_bytes();
			end[..2].copy_from_slice(&checksum);
			let mut with_end = input.to_vec();
			with_end.extend_from_slice(&end);
			assert_eq!(list_members(&with_end).unwrap().len(), 3);
			with_end.pop();
			assert!(matches!(list_members(&with_end), Err(Error::Truncated(_))));
		}
	}

	#[test]
	fn rejects_corrupt_member_crc() {
		let mut corrupt = fixture("rar50-normal").to_vec();
		let members = list_members(&corrupt).unwrap();
		assert_eq!(members[0].index, 0);
		// The first RAR5 file header stores its output CRC in the type-specific
		// fields. Flipping payload bytes is enough to exercise output checking.
		let archive = parse_archive(&corrupt).unwrap();
		let byte = archive.files[0].packed.start;
		corrupt[byte] ^= 1;
		assert!(matches!(
			extract_member(&corrupt, 0),
			Err(Error::Decoder(_)) | Err(Error::ChecksumMismatch(_))
		));
	}

	#[test]
	fn enforces_member_and_archive_output_limits() {
		assert!(check_member_size(MAX_MEMBER_UNPACKED_BYTES).is_ok());
		assert!(matches!(
			check_member_size(MAX_MEMBER_UNPACKED_BYTES + 1),
			Err(Error::LimitExceeded(_))
		));
		assert!(matches!(
			checked_total(MAX_TOTAL_UNPACKED_BYTES - 1, 2),
			Err(Error::LimitExceeded(_))
		));
	}

	#[test]
	fn rejects_rar3_ppmd_before_decoder_creation() {
		assert!(matches!(
			reject_initial_rar3_ppmd(&[0x80], 0..1),
			Err(Error::Unsupported(_))
		));
		assert!(reject_initial_rar3_ppmd(&[0x00], 0..1).is_ok());
	}

	#[test]
	fn for_each_member_matches_extract_member() {
		for archive_name in ["rar40-normal", "rar40-solid", "rar50-normal", "rar50-solid"] {
			let input = fixture(archive_name);
			let mut visited = Vec::new();
			for_each_member(input, |index, output| {
				visited.push((index, output.to_vec()))
			})
			.unwrap();
			assert_eq!(visited.len(), 3, "{archive_name}");
			for (index, output) in visited {
				assert_eq!(
					output,
					extract_member(input, index).unwrap(),
					"{archive_name}"
				);
			}
		}
	}

	#[test]
	#[ignore = "large fixtures are generated artifacts"]
	fn extracts_large_fixtures() {
		let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("../../experiments/cbr-wasm/artifacts/large");
		assert!(
			base.join("rar40-normal.cbr").is_file(),
			"generate large fixtures first; see README.md"
		);
		let expected = [
			std::fs::read(base.join("page10.png")).unwrap(),
			std::fs::read(base.join("page2.png")).unwrap(),
			std::fs::read(base.join("page1.png")).unwrap(),
		];
		for archive_name in [
			"rar40-normal.cbr",
			"rar40-solid.cbr",
			"rar50-normal.cbr",
			"rar50-solid.cbr",
		] {
			let input = std::fs::read(base.join(archive_name)).unwrap();
			let members = list_members(&input).unwrap();
			assert_eq!(members.len(), expected.len(), "{archive_name}");
			for (index, expected) in expected.iter().enumerate() {
				assert_eq!(members[index].size, expected.len() as u64, "{archive_name}");
				let output = extract_member(&input, index).unwrap();
				assert_eq!(
					output.as_slice(),
					expected.as_slice(),
					"{archive_name} {index}"
				);
			}
		}
	}
}
