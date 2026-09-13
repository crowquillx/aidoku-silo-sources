use compcol::{Decoder, Status};
fn decode(dec: &mut dyn Decoder, input: &[u8], size: usize) -> Result<Vec<u8>, String> {
	let mut out = vec![0; size];
	let (mut read, mut written) = (0, 0);
	for _ in 0..100 {
		let (p, status) = if read < input.len() {
			dec.decode(&input[read..], &mut out[written..])
		} else {
			dec.finish(&mut out[written..])
		}
		.map_err(|e| format!("{e:?}"))?;
		read += p.consumed;
		written += p.written;
		if status == Status::StreamEnd {
			return if written == size {
				Ok(out)
			} else {
				Err(format!("short {written}/{size}"))
			};
		}
		if p.consumed == 0 && p.written == 0 {
			return Err(format!("stalled {written}/{size}"));
		}
	}
	Err("step limit".into())
}
fn main() {
	let dir = std::env::args().nth(1).unwrap();
	for family in ["rar40", "rar50"] {
		for solid in ["normal", "solid"] {
			let name = format!("{family}-{solid}.cbr");
			let bytes = std::fs::read(format!("{dir}/{name}")).unwrap();
			match rars::ArchiveReader::read(&bytes).unwrap() {
				rars::Archive::Rar15To40(a) => {
					let files: Vec<_> = a.files().collect();
					let mut solid_dec =
						compcol::rar3::Decoder::with_unpack_size(files[0].unp_size).with_solid();
					for (i, f) in files.iter().enumerate() {
						let payload = &bytes[f.packed_range.clone()];
						let mut independent = compcol::rar3::Decoder::with_unpack_size(f.unp_size);
						if i > 0 && solid == "solid" && !f.is_stored() {
							solid_dec.begin_solid_member(f.unp_size).unwrap();
						}
						let dec: &mut dyn Decoder = if solid == "solid" {
							&mut solid_dec
						} else {
							&mut independent
						};
						let out = if f.is_stored() {
							Ok(payload.to_vec())
						} else {
							decode(dec, payload, f.unp_size as usize)
						};
						let expected =
							std::fs::read(format!("{dir}/{}", String::from_utf8_lossy(&f.name)))
								.unwrap();
						println!(
							"{name} {i} method={} ver={} compressed={} result={:?}",
							f.method,
							f.unp_ver,
							payload.len(),
							out.as_ref().map(|v| (v.len(), *v == expected))
						);
						assert!(
							matches!(&out, Ok(v) if v == &expected),
							"fixture bytes differ"
						);
						if out.is_err() {
							break;
						}
					}
				}
				rars::Archive::Rar50Plus(a) => {
					let files: Vec<_> = a.files().collect();
					if solid == "solid" {
						let size = files.iter().map(|f| f.unpacked_size).sum();
						let mut packed = Vec::new();
						let mut expected = Vec::new();
						let window = files
							.iter()
							.map(|f| 128 * 1024 << ((f.compression_info >> 10) & 15))
							.max()
							.unwrap();
						println!("{name} window={window}");
						let mut dec =
							compcol::rar5::Decoder::with_unpack_size_and_window(size, window);
						for f in &files {
							dec.add_file_boundary(expected.len() as u64);
							packed.extend_from_slice(&bytes[f.block.data_range.clone()]);
							expected.extend(
								std::fs::read(format!(
									"{dir}/{}",
									String::from_utf8_lossy(&f.name)
								))
								.unwrap(),
							);
						}
						let out = decode(&mut dec, &packed, size as usize);
						println!(
							"{name} solid group result={:?}",
							out.as_ref().map(|v| (v.len(), *v == expected))
						);
						assert!(
							matches!(&out, Ok(v) if v == &expected),
							"fixture bytes differ"
						);
					} else {
						for (i, f) in files.iter().enumerate() {
							let payload = &bytes[f.block.data_range.clone()];
							let mut dec = compcol::rar5::Decoder::with_unpack_size_and_window(
								f.unpacked_size,
								128 * 1024 << ((f.compression_info >> 10) & 15),
							);
							let out = if f.is_stored() {
								Ok(payload.to_vec())
							} else {
								decode(&mut dec, payload, f.unpacked_size as usize)
							};
							let expected = std::fs::read(format!(
								"{dir}/{}",
								String::from_utf8_lossy(&f.name)
							))
							.unwrap();
							println!(
								"{name} {i} ci={} compressed={} result={:?}",
								f.compression_info,
								payload.len(),
								out.as_ref().map(|v| (v.len(), *v == expected))
							);
							assert!(
								matches!(&out, Ok(v) if v == &expected),
								"fixture bytes differ"
							);
						}
					}
				}
				_ => {}
			}
		}
	}
}
