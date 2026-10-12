// Generates src-tauri/resources/THIRD-PARTY-NOTICES.txt: the license text of every library that
// ships inside the desktop app (Rust runtime dependencies + npm packages bundled into the UI).
// MIT/Apache/BSD require these notices to travel with copies of the software.
// Run: npm run notices   (re-run after adding or upgrading dependencies)
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const out = path.join(root, 'src-tauri/resources/THIRD-PARTY-NOTICES.txt');
const LICENSE_FILE = /^(licen[cs]e|copying|notice|copyright|unlicense)([-._].*)?$/i;

function licenseTexts(dir) {
  let names = [];
  try { names = fs.readdirSync(dir); } catch { return []; }
  return names.filter(n => LICENSE_FILE.test(n)).sort()
    .map(n => path.join(dir, n)).filter(p => fs.statSync(p).isFile())
    .map(p => fs.readFileSync(p, 'utf8').replace(/\r\n/g, '\n').trim());
}

// ---- Rust: walk normal (runtime) dependencies from the app crates; build/dev deps don't ship.
function rustPackages() {
  const meta = JSON.parse(execFileSync('cargo', ['metadata', '--format-version', '1', '--locked'],
    { cwd: root, maxBuffer: 1 << 28, encoding: 'utf8' }));
  const pkgs = new Map(meta.packages.map(p => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map(n => [n.id, n]));
  const members = new Set(meta.workspace_members);
  const seen = new Set(); const stack = [...members];
  while (stack.length) {
    const id = stack.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const d of nodes.get(id)?.deps ?? []) {
      if (d.dep_kinds.some(k => k.kind === null)) stack.push(d.pkg);
    }
  }
  return [...seen].filter(id => !members.has(id)).map(id => pkgs.get(id)).map(p => ({
    name: p.name, version: p.version, license: p.license || (p.license_file ? 'see file' : 'unspecified'),
    url: p.repository || p.homepage || `https://crates.io/crates/${p.name}`,
    texts: licenseTexts(path.dirname(p.manifest_path)),
  }));
}

// ---- npm: the UI's imports and their dependency trees (resolved through node_modules).
function npmPackages() {
  const roots = new Set(['@fontsource-variable/inter']);
  const walk = d => fs.readdirSync(d, { withFileTypes: true }).forEach(e => {
    const p = path.join(d, e.name);
    if (e.isDirectory()) return walk(p);
    if (!/\.(js|vue|css)$/.test(e.name)) return;
    const src = fs.readFileSync(p, 'utf8');
    for (const m of src.matchAll(/(?:from\s+|import\s*\(\s*|@import\s+)['"]([^'"./][^'"]*)['"]/g)) {
      const spec = m[1].startsWith('@') ? m[1].split('/').slice(0, 2).join('/') : m[1].split('/')[0];
      roots.add(spec);
    }
  });
  walk(path.join(root, 'web/src'));
  const seen = new Map(); const stack = [...roots];
  while (stack.length) {
    const name = stack.pop();
    if (seen.has(name)) continue;
    const dir = path.join(root, 'node_modules', name);
    let pj;
    try { pj = JSON.parse(fs.readFileSync(path.join(dir, 'package.json'), 'utf8')); } catch { continue; }
    const lic = typeof pj.license === 'string' ? pj.license : pj.license?.type ?? (pj.licenses || []).map(l => l.type).join(' OR ');
    const repo = typeof pj.repository === 'string' ? pj.repository : pj.repository?.url;
    seen.set(name, { name, version: pj.version, license: lic || 'unspecified', url: pj.homepage || repo || `https://www.npmjs.com/package/${name}`, texts: licenseTexts(dir) });
    stack.push(...Object.keys(pj.dependencies || {}));
  }
  return [...seen.values()];
}

// Identical license texts (e.g. the Apache-2.0 body) are printed once, with every package that uses it.
function render(groups) {
  const lines = [
    'Vault-LM includes the open-source software listed below. Each component is used under its own',
    'license, reproduced here. Vault-LM itself is MIT-licensed (see LICENSE).',
    '',
  ];
  for (const [title, list] of groups) {
    list.sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version, undefined, { numeric: true }));
    lines.push('='.repeat(78), title, '='.repeat(78), '');
    for (const p of list) lines.push(`${p.name} ${p.version}  (${p.license})  ${p.url}`);
    lines.push('');
    const byText = new Map();
    for (const p of list) {
      if (!p.texts.length) continue;
      const key = p.texts.join('\n\n----\n\n');
      if (!byText.has(key)) byText.set(key, []);
      byText.get(key).push(`${p.name} ${p.version}`);
    }
    for (const [text, users] of byText) {
      lines.push('-'.repeat(78), `Used by: ${users.join(', ')}`, '-'.repeat(78), text, '');
    }
    const bare = list.filter(p => !p.texts.length);
    if (bare.length) {
      lines.push('-'.repeat(78), 'These packages ship no license file; they are used under the SPDX license named above:',
        bare.map(p => `${p.name} ${p.version} (${p.license})`).join(', '), '');
    }
  }
  // Packages without a license file still need the license text: append one canonical copy of each
  // standard license, taken from a package that does ship it.
  const all = groups.flatMap(([, list]) => list);
  const standard = [
    ['Apache-2.0', t => /Apache License\s+Version 2\.0, January 2004/.test(t) && /END OF TERMS AND CONDITIONS/.test(t)],
    ['MIT', t => /^(MIT License|The MIT License)?[\s\S]{0,300}Permission is hereby granted, free of charge/.test(t) && t.length < 1600],
    ['BSD-3-Clause', t => /Redistribution and use in source and binary forms/.test(t) && /Neither the name/.test(t) && t.length < 2500],
    ['BSD-2-Clause', t => /Redistribution and use in source and binary forms/.test(t) && !/Neither the name/.test(t) && t.length < 2000],
    ['ISC', t => /Permission to use, copy, modify, and\/or distribute this software/.test(t) && t.length < 1200],
    ['Zlib', t => /This software is provided 'as-is'/.test(t) && t.length < 1500],
    ['Unicode-3.0', t => /UNICODE LICENSE V3/.test(t)],
  ];
  const needed = new Set(all.filter(p => !p.texts.length).flatMap(p => p.license.split(/\s+(?:OR|AND|WITH)\s+|[()/]/)).map(x => x.trim()).filter(Boolean));
  const appendix = standard.filter(([id]) => needed.has(id)).map(([id, test]) => {
    const text = all.flatMap(p => p.texts.flatMap(t => t.split('\n\n----\n\n'))).find(test);
    return text && `--- ${id} ---\n${text}`;
  }).filter(Boolean);
  if (appendix.length) {
    lines.push('='.repeat(78), 'Standard license texts for the packages listed above without their own license file. Copyright',
      'lines inside these texts belong to the package each copy was taken from; the terms are identical.',
      '='.repeat(78), '', ...appendix.flatMap(a => [a, '']));
  }
  // PDFium is bundled as a binary, not a crate; CI copies its license next to the library.
  const pdfium = path.join(root, 'src-tauri/resources/pdfium/LICENSE-pdfium');
  lines.push('='.repeat(78), 'PDFium (bundled library, https://pdfium.googlesource.com/pdfium/)', '='.repeat(78), '',
    fs.existsSync(pdfium) ? fs.readFileSync(pdfium, 'utf8').trim()
      : 'BSD-3-Clause. The full license is shipped next to the library as pdfium/LICENSE-pdfium.', '');
  return lines.join('\n') + '\n';
}

const rust = rustPackages(); const npm = npmPackages();
fs.writeFileSync(out, render([['Rust libraries', rust], ['JavaScript libraries (user interface)', npm]]));
const missing = [...rust, ...npm].filter(p => !p.texts.length).length;
console.log(`${path.relative(root, out)}: ${rust.length} Rust + ${npm.length} npm packages (${missing} without a license file), ${(fs.statSync(out).size / 1024).toFixed(0)} KB`);
