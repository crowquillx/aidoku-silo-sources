#![no_std]
extern crate alloc;
use aidoku as _;
use alloc::vec::Vec;
use wasmi::{Engine, Linker, Module, Store};

#[unsafe(no_mangle)]
pub extern "C" fn wasmi_probe() -> u32 {
	let wasm: &[u8] = b"\0asm\x01\0\0\0\x01\x05\x01\x60\0\x01\x7f\x03\x02\x01\0\x07\x0a\x01\x06answer\0\0\x0a\x06\x01\x04\0\x41\x2a\x0b";
	let engine = Engine::default();
	let module = Module::new(&engine, wasm).unwrap();
	let mut store = Store::new(&engine, ());
	let instance = Linker::new(&engine)
		.instantiate_and_start(&mut store, &module)
		.unwrap();
	instance
		.get_typed_func::<(), u32>(&store, "answer")
		.unwrap()
		.call(&mut store, ())
		.unwrap()
}

pub fn call_rar(input: &[u8], index: i32, fuel: u64) -> Result<Vec<u8>, wasmi::Error> {
	let mut config = wasmi::Config::default();
	config.consume_fuel(true);
	let engine = Engine::new(&config);
	let module = Module::new(&engine, include_bytes!("../artifacts/rar-guest.wasm"))?;
	assert_eq!(module.imports().count(), 0);
	let limits = wasmi::StoreLimitsBuilder::new()
		.memory_size(128 * 1024 * 1024)
		.build();
	let mut store = Store::new(&engine, limits);
	store.limiter(|limits| limits);
	store.set_fuel(fuel)?;
	let instance = Linker::new(&engine).instantiate_and_start(&mut store, &module)?;
	let memory = instance.get_memory(&store, "memory").unwrap();
	let ptr = instance
		.get_typed_func::<u32, u32>(&store, "input_alloc")?
		.call(&mut store, input.len() as u32)?;
	memory.write(&mut store, ptr as usize, input)?;
	let run = instance.get_typed_func::<(u32, u32, i32), u64>(&store, "rar_call")?;
	let result = run.call(&mut store, (ptr, input.len() as u32, index));
	aidoku::println!(
		"RAR input={} index={} guest_memory={} fuel={}",
		input.len(),
		index,
		memory.data_size(&store),
		fuel - store.get_fuel()?
	);
	let packed = result?;
	let mut out = alloc::vec![0; (packed>>32) as usize];
	memory.read(&store, packed as u32 as usize, &mut out)?;
	Ok(out)
}

#[unsafe(no_mangle)]
pub extern "C" fn probe_alloc(len: u32) -> u32 {
	alloc::boxed::Box::into_raw(alloc::vec![0u8;len as usize].into_boxed_slice()) as *mut u8 as u32
}

/// # Safety
/// `ptr` must come from `probe_alloc(len)` and must not have been consumed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn probe_run(ptr: u32, len: u32, index: i32, fuel: u64) -> u32 {
	let input = unsafe { Vec::from_raw_parts(ptr as *mut u8, len as usize, len as usize) };
	match call_rar(&input, index, fuel) {
		Ok(out) => out.len() as u32,
		Err(e) => {
			aidoku::println!("guest failure: {e}");
			u32::MAX
		}
	}
}

#[unsafe(no_mangle)]
pub extern "C" fn rar_fixture_benchmark(which: u32, index: i32) -> u32 {
	let fixtures: [&[u8]; 4] = [
		include_bytes!("../fixtures/rar40-normal.cbr"),
		include_bytes!("../fixtures/rar40-solid.cbr"),
		include_bytes!("../fixtures/rar50-normal.cbr"),
		include_bytes!("../fixtures/rar50-solid.cbr"),
	];
	let out = call_rar(fixtures[which as usize], index, 500_000_000).unwrap();
	if index >= 0 {
		assert_eq!(out.as_slice(), include_bytes!("../fixtures/page1.png"));
	}
	out.len() as u32
}

#[cfg(test)]
mod tests {
	use super::*;
	use aidoku_test::aidoku_test;
	#[aidoku_test]
	fn wasm_in_wasm() {
		assert_eq!(wasmi_probe(), 42);
	}
	#[aidoku_test]
	fn real_rar4_and_rar5() {
		for which in 0..4 {
			rar_fixture_benchmark(which, -1);
			rar_fixture_benchmark(which, 2);
		}
	}
	#[aidoku_test]
	fn npm_modules_validate() {
		for (name, wasm) in [
			(
				"rars-0.9.4",
				include_bytes!("../artifacts/rars-npm.wasm").as_slice(),
			),
			(
				"node-unrar-js-2.0.2",
				include_bytes!("../artifacts/unrar-npm.wasm").as_slice(),
			),
		] {
			let engine = Engine::default();
			let module = Module::new(&engine, wasm).unwrap();
			aidoku::println!(
				"{} bytes={} imports={}",
				name,
				wasm.len(),
				module.imports().count()
			);
			for e in module.exports() {
				if let wasmi::ExternType::Memory(ty) = e.ty() {
					aidoku::println!("memory {:?}", ty);
				}
			}
			let mut store = Store::new(&engine, ());
			let result = Linker::new(&engine).instantiate_and_start(&mut store, &module);
			aidoku::println!("without JS glue: {}", result.unwrap_err());
		}
	}
}
