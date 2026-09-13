const fs = require('node:fs');
const path = require('node:path');
const { createExtractorFromData } = require('node-unrar-js');
const dir = path.resolve(__dirname, process.argv.includes('--large') ? 'artifacts/large' : 'fixtures');
(async () => {
  for (const name of fs.readdirSync(dir).filter(x => x.endsWith('.cbr'))) {
    const data = Uint8Array.from(fs.readFileSync(path.join(dir, name))).buffer;
    const extractor = await createExtractorFromData({data});
    const files = [...extractor.extract().files];
    if(files.length !== 3) throw Error('expected three pages');
    for (const file of files) {
      const expected = fs.readFileSync(path.join(dir, file.fileHeader.name));
      if (!Buffer.from(file.extraction).equals(expected)) throw Error(name + ': bytes differ');
    }
    console.log(name, 'UnRAR verified all three PNGs');
  }
})().catch(error => {console.error(error);process.exitCode = 1;});
