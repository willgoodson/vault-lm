import { loadVault } from './vaults.js';
import { indexVault } from './indexer.js';
await loadVault();
console.log(await indexVault());
