// SD-2b prepare: one case directory. Confirms the pinned runtime exists and writes map.json.
// The bundle bytes are already the harness emitter's output. This file does not compile.
import fs from 'node:fs';
import path from 'node:path';

const dir = path.resolve(process.argv[2]);
const input = JSON.parse(fs.readFileSync(path.join(dir, 'input.json'), 'utf8'));
if (input.schema !== 'semdiff-exec-input/v0' || input.engine !== 'ssr-runtime') {
    console.error(`ssr-prepare: input engine ${input.engine} schema ${input.schema}`);
    process.exit(1);
}
const runtime = process.env.SD_SSR_RUNTIME;
if (!runtime || !fs.existsSync(runtime)) {
    console.error(`ssr-prepare: pinned runtime missing (${runtime})`);
    process.exit(1);
}
const map = { engine: 'ssr-runtime', runtime, mode: input.mode };
fs.writeFileSync(path.join(dir, 'map.json'), JSON.stringify(map));
