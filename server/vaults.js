import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { DATA } from './config.js';

const stateFile = path.join(DATA, 'state.json');
const OBSIDIAN_JSON = path.join(os.homedir(), 'Library/Application Support/obsidian/obsidian.json');

export const slug = (p) => p.replace(/^\/+/, '').replace(/[^a-zA-Z0-9]+/g, '-').slice(-80);

// Mutable current-vault state shared by every module.
export const vault = { name: '', path: '', data: '' };

export function setVault(p) {
  vault.path = path.resolve(p);
  vault.name = path.basename(vault.path);
  vault.data = path.join(DATA, 'vaults', slug(vault.path));
  return vault;
}

export async function loadVault() {
  const saved = JSON.parse(await fs.readFile(stateFile, 'utf8').catch(() => '{}'));
  setVault(process.env.VAULT || saved.vault || path.join(os.homedir(), 'Documents/Notebooks'));
  await fs.mkdir(vault.data, { recursive: true });
  return vault;
}

export async function saveVault() {
  await fs.mkdir(DATA, { recursive: true });
  const saved = JSON.parse(await fs.readFile(stateFile, 'utf8').catch(() => '{}'));
  const vaults = [...new Set([vault.path, ...(saved.vaults || [])])];
  await fs.writeFile(stateFile, JSON.stringify({ vault: vault.path, vaults }, null, 2));
}

// Vaults Vault-LM has opened, plus ones Obsidian knows about (its config is read, never written).
export async function knownVaults() {
  const saved = JSON.parse(await fs.readFile(stateFile, 'utf8').catch(() => '{}'));
  const cfg = JSON.parse(await fs.readFile(OBSIDIAN_JSON, 'utf8').catch(() => '{"vaults":{}}'));
  const fromObsidian = Object.values(cfg.vaults || {}).map((v) => v.path);
  const seen = new Map();
  for (const p of [...(saved.vaults || []), ...fromObsidian]) {
    if (!seen.has(p)) seen.set(p, { name: path.basename(p), path: p, fromObsidian: !(saved.vaults || []).includes(p) });
  }
  return [...seen.values()];
}

export async function isIndexed(p) {
  return !!(await fs.stat(path.join(DATA, 'vaults', slug(p), 'lancedb')).catch(() => null));
}
