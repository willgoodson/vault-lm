import fs from 'node:fs/promises';
import path from 'node:path';
import { KokoroTTS } from 'kokoro-js';
import { vault } from './vaults.js';
import { podcastScript, studioPath } from './llm.js';
import { writeNote } from './files.js';

const VOICES = { A: 'af_heart', B: 'am_michael' };
let ttsPromise;
const tts = () => (ttsPromise ??= KokoroTTS.from_pretrained('onnx-community/Kokoro-82M-v1.0-ONNX', { dtype: 'q8', device: 'cpu' }));

function wav(samples, rate) {
  const buf = Buffer.alloc(44 + samples.length * 2);
  buf.write('RIFF', 0); buf.writeUInt32LE(36 + samples.length * 2, 4); buf.write('WAVE', 8);
  buf.write('fmt ', 12); buf.writeUInt32LE(16, 16); buf.writeUInt16LE(1, 20); buf.writeUInt16LE(1, 22);
  buf.writeUInt32LE(rate, 24); buf.writeUInt32LE(rate * 2, 28); buf.writeUInt16LE(2, 32); buf.writeUInt16LE(16, 34);
  buf.write('data', 36); buf.writeUInt32LE(samples.length * 2, 40);
  for (let i = 0; i < samples.length; i++) buf.writeInt16LE(Math.max(-1, Math.min(1, samples[i])) * 32767, 44 + i * 2);
  return buf;
}

export async function audioOverview(scope, onProgress = () => {}) {
  onProgress('Writing script…');
  const lines = await podcastScript(scope);
  if (!lines.length) throw new Error('Model returned no script lines');
  const model = await tts();
  const parts = [];
  let rate = 24000;
  for (const [i, l] of lines.entries()) {
    onProgress(`Voicing line ${i + 1}/${lines.length}`);
    const a = await model.generate(l.text, { voice: VOICES[l.speaker] });
    rate = a.sampling_rate;
    parts.push(a.audio, new Float32Array(Math.round(rate * 0.35)));
  }
  const all = new Float32Array(parts.reduce((n, p) => n + p.length, 0));
  let off = 0;
  for (const p of parts) { all.set(p, off); off += p.length; }
  const scriptPath = studioPath(scope, 'Audio Overview');
  const wavRel = scriptPath.replace(/\.md$/, '.wav');
  await fs.mkdir(path.join(vault.path, path.dirname(wavRel)), { recursive: true });
  await fs.writeFile(path.join(vault.path, wavRel), wav(all, rate));
  const script = lines.map((l) => `**${l.speaker}:** ${l.text}`).join('\n\n');
  await writeNote(scriptPath, `![[${path.basename(wavRel)}]]\n\n${script}\n`);
  return { path: wavRel, script: scriptPath, lines: lines.length };
}
