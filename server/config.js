import path from 'node:path';

export const DATA = path.resolve(import.meta.dirname, '../data');
// Chat/embedding models are configured in data/models.json (see models.js); env CHAT_MODEL / EMBED_MODEL set the defaults.
export const K_PRIMARY = 8;
export const K_REFERENCE = 5;
// Cosine distance cutoff for outside-notebook chunks in reference mode (lower = stricter).
export const REF_MAX_DISTANCE = 0.42;
// Generated output folder inside each notebook; never indexed.
export const STUDIO = '_studio';
export const SKIP_DIRS = new Set([STUDIO, '_briefs']);
