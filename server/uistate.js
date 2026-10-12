// Per-vault UI state (per-notebook tabs, selection, chats) so it survives reloads and browsers.
// Writes are serialized and atomic (unique temp file + rename), so concurrent saves can't interleave.
import fs from 'node:fs/promises';
import path from 'node:path';
import { vault } from './vaults.js';

const file = () => path.join(vault.data, 'ui-state.json');
export async function loadUiState() {
  const raw = await fs.readFile(file(), 'utf8').catch(() => '{}');
  try { return JSON.parse(raw); } catch { return {}; }
}
let queue = Promise.resolve();
export function saveUiState(state) {
  const target = file();
  queue = queue.then(async () => {
    await fs.mkdir(path.dirname(target), { recursive: true });
    const tmp = `${target}.${process.pid}.${Date.now()}.${Math.random().toString(36).slice(2)}.tmp`;
    await fs.writeFile(tmp, JSON.stringify(state));
    await fs.rename(tmp, target);
  }).catch((e) => console.error('ui-state save failed:', e.message));
  return queue;
}
