const fs = require('fs');
const path = require('path');
const large = process.argv.includes('--large');
const fixtureDir = path.join(__dirname, large ? 'artifacts/large' : 'fixtures');
const zlib = require('zlib');
const {RarWriter} = require('@bitplane/rars');
function crc32(buf) { let c=0xffffffff; for(const b of buf){c^=b;for(let i=0;i<8;i++)c=(c>>>1)^((c&1)?0xedb88320:0);}return (c^0xffffffff)>>>0; }
function chunk(type,data){let t=Buffer.from(type),len=Buffer.alloc(4),crc=Buffer.alloc(4);len.writeUInt32BE(data.length);crc.writeUInt32BE(crc32(Buffer.concat([t,data])));return Buffer.concat([len,t,data,crc]);}
function png(w,h,seed){let hdr=Buffer.alloc(13);hdr.writeUInt32BE(w);hdr.writeUInt32BE(h,4);hdr[8]=8;hdr[9]=2;let data=Buffer.alloc(h*(1+w*3));for(let y=0;y<h;y++)for(let x=0;x<w;x++){let i=y*(1+w*3)+1+x*3;if(large){seed^=seed<<13;seed^=seed>>>17;seed^=seed<<5;data[i]=seed&255;data[i+1]=(seed>>>8)&255;data[i+2]=(seed>>>16)&255;}else{data[i]=(x+seed)%256;data[i+1]=(y+seed)%256;data[i+2]=((x^y)+seed)%256;}}return Buffer.concat([Buffer.from('89504e470d0a1a0a','hex'),chunk('IHDR',hdr),chunk('IDAT',zlib.deflateSync(data,{level:0})),chunk('IEND',Buffer.alloc(0))]);}
(async()=>{fs.mkdirSync(fixtureDir,{recursive:true});for(const format of ['rar40','rar50'])for(const solid of [false,true]){const writer=new RarWriter({format,solid,level:3});for(const n of [10,2,1]){let data=png(large?512:128,large?768:128,n);writer.add(`page${n}.png`,data,{modifiedAt:new Date('2024-01-01T00:00:00Z')});fs.writeFileSync(`${fixtureDir}/page${n}.png`,data);}let bytes=await writer.bytes();let name=`${format}-${solid?'solid':'normal'}.cbr`;fs.writeFileSync(`${fixtureDir}/${name}`,bytes);console.log(name,bytes.length);writer.close();}process.exit(0);})().catch(e=>{console.error(e);process.exit(1)});
