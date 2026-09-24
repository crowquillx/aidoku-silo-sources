#![no_std]
extern crate alloc;
use aidoku as _;
use alloc::vec::Vec;

fn run(input: &[u8], index: i32) -> Vec<u8> {
	if index < 0 {
		return alloc::format!("{:?}", silo_cbr_native::list_members(input).unwrap().len())
			.into_bytes();
	}
	silo_cbr_native::extract_member(input, index as usize).unwrap()
}
#[unsafe(no_mangle)]
pub extern "C" fn rar_fixture_benchmark(which: u32, index: i32) -> u32 {
	let input: [&[u8]; 4] = [
		include_bytes!("../../../crates/cbr-native/fixtures/rar40-normal.cbr"),
		include_bytes!("../../../crates/cbr-native/fixtures/rar40-solid.cbr"),
		include_bytes!("../../../crates/cbr-native/fixtures/rar50-normal.cbr"),
		include_bytes!("../../../crates/cbr-native/fixtures/rar50-solid.cbr"),
	];
	let out = run(input[which as usize], index);
	if index >= 0 {
		assert_eq!(
			out.as_slice(),
			include_bytes!("../../../crates/cbr-native/fixtures/page1.png")
		);
	}
	out.len() as u32
}
#[unsafe(no_mangle)]
pub extern "C" fn probe_alloc(len: u32) -> u32 {
	alloc::boxed::Box::into_raw(alloc::vec![0u8;len as usize].into_boxed_slice()) as *mut u8 as u32
}
/// # Safety
/// `ptr` must come from `probe_alloc(len)` and not have been consumed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn probe_run(ptr: u32, len: u32, index: i32, _fuel: u64) -> u32 {
	let input = unsafe { Vec::from_raw_parts(ptr as *mut u8, len as usize, len as usize) };
	let out = run(&input, index);
	out.len() as u32
}
#[cfg(test)]
mod tests {
	use aidoku_test::aidoku_test;
	#[aidoku_test]
	fn real_fixtures() {
		for i in 0..4 {
			super::rar_fixture_benchmark(i, -1);
			super::rar_fixture_benchmark(i, 2);
		}
	}
}
