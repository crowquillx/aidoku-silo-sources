import fs from 'node:fs';
const module=new WebAssembly.Module(fs.readFileSync('target/wasm32-unknown-unknown/release/silo_cbr_wasm_probe.wasm'));
let instance;
instance=new WebAssembly.Instance(module,{env:{print:(p,n)=>console.log(new TextDecoder().decode(new Uint8Array(instance.exports.memory.buffer,p,n))),abort:()=>{throw Error('source aborted')}}});
for(const name of fs.readdirSync('artifacts/large').filter(x=>x.endsWith('.cbr'))){
for(const fuel of [500_000_000n,12_000_000_000n]){
const data=fs.readFileSync('artifacts/large/'+name),ptr=instance.exports.probe_alloc(data.length);
new Uint8Array(instance.exports.memory.buffer,ptr,data.length).set(data);
const start=performance.now();const size=instance.exports.probe_run(ptr,data.length,2,fuel);
console.log(JSON.stringify({name,fuel:String(fuel),result:size,ms:performance.now()-start,outer_memory:instance.exports.memory.buffer.byteLength}));
}
}
