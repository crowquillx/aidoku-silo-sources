import fs from 'node:fs';
const file=process.argv[2]??'target/wasm32-unknown-unknown/release/silo_cbr_wasm_probe.wasm';
const bytes=fs.readFileSync(file),module=new WebAssembly.Module(bytes);
let instance;
const imports={env:{print:(p,n)=>console.log(new TextDecoder().decode(new Uint8Array(instance.exports.memory.buffer,p,n))),abort:()=>{throw Error('source aborted')}}};
instance=new WebAssembly.Instance(module,imports);
console.log(JSON.stringify({engine:process.version,wasm_bytes:bytes.length,imports:WebAssembly.Module.imports(module),initial_memory:instance.exports.memory.buffer.byteLength}));
for(let pass=0;pass<2;pass++)for(let fixture=0;fixture<4;fixture++)for(const entry of [-1,2]){
const start=performance.now();const size=instance.exports.rar_fixture_benchmark(fixture,entry);
console.log(JSON.stringify({pass,fixture,entry,bytes:size,ms:performance.now()-start,outer_memory:instance.exports.memory.buffer.byteLength}));
}
