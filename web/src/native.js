// Native integrations for the desktop app (no-ops / browser fallbacks otherwise).
import { isDesktop } from './api.js';

export async function openExternal(url) {
  if (!isDesktop) return void window.open(url, '_blank', 'noopener');
  const { openUrl } = await import('@tauri-apps/plugin-opener');
  return openUrl(url);
}

export async function pickFolder(title = 'Choose a folder') {
  if (!isDesktop) return prompt(`${title} (path)`);
  const { open } = await import('@tauri-apps/plugin-dialog');
  return open({ directory: true, multiple: false, title });
}

export async function pickNewFolder(title = 'Create a new vault', defaultName = 'My Vault') {
  if (!isDesktop) return prompt(`${title} (path)`, `~/Documents/${defaultName}`);
  const { save } = await import('@tauri-apps/plugin-dialog');
  const { documentDir, join } = await import('@tauri-apps/api/path');
  return save({ title, defaultPath: await join(await documentDir(), defaultName) });
}

// In the desktop app, links that would open a new browser tab go to the system browser instead.
export function routeExternalLinks() {
  if (!isDesktop) return;
  window.open = (url) => { if (url) openExternal(String(url)); return null; };
  document.addEventListener('click', (e) => {
    const a = e.target.closest?.('a[href]');
    if (!a) return;
    const href = a.getAttribute('href');
    if (/^(https?:|mailto:|obsidian:)/i.test(href)) { e.preventDefault(); openExternal(href); }
  }, true);
}
