# Developing Vault-LM

## Architecture

Vault-LM is a [Tauri v2](https://tauri.app) desktop app:

```
crates/vaultlm-core/   all app logic: a plain Rust library, unit-tested
src-tauri/             the desktop shell: window, native menus, IPC commands, bundling
web/                   the user interface (Vue 3 + Vite + Tailwind), shown in the system WebView
server/                the original Node implementation, kept as a reference (see below)
scripts/               maintenance scripts (third-party notices)
examples/demo-vault/   sample notes for trying the app and taking screenshots
```

- **No local server and no open ports.** The UI calls the core over Tauri IPC through one generic command, `api(method, path, query, body)`. It dispatches to handlers that mirror REST-style routes (`/api/tree`, `/api/chat`, …). `web/src/api.js` hides the transport: inside Tauri it uses `invoke`, and in a plain browser it uses `fetch`. That lets the same UI run against the test harness.
- **Streaming** (chat tokens) uses a Tauri `Channel`. **Live updates** (indexing progress, file changes, jobs) are Tauri events, received with `listen()` in `web/src/store.js`.
- **Files** shown in the UI (images, audio) are loaded through Tauri's asset protocol, scoped to the open vault.

### Main pieces of the core

| Module | Responsibility |
|---|---|
| `files`, `layout`, `links` | reading and writing the vault, the notebook/section/topic tree, wikilink resolution and rewriting on rename |
| `store`, `vaults`, `watcher` | app state, the vault list, and file watching (`notify`) |
| `index`, `retrieve` | LanceDB vector + full-text index, and hybrid retrieval with reciprocal-rank fusion |
| `models`, `ai` | Ollama, Anthropic (raw HTTP) and OpenAI-compatible providers, plus chat and the Studio features |
| `srs`, `progress` | spaced repetition (SM-2), and the review statistics |
| `sources`, `pdfmd` | importing web pages, PDFs (PDFium), EPUB, images (ocrs OCR) and YouTube |
| `audio` | audio overviews with the OS's speech engine |
| `api` | the route dispatcher used by IPC and the test harness |

## Prerequisites

- **Rust** (stable) and **Node 22**
- **protoc**, the Protocol Buffers compiler, which LanceDB needs to build: `brew install protobuf` on macOS, `apt install protobuf-compiler` on Debian/Ubuntu
- **Linux only:** `libdbus-1-dev` and `pkg-config` (for the keychain), plus the WebKitGTK development packages for the app itself (see the list in `.github/workflows/release.yml`)
- **PDFium**, the PDF library, in `src-tauri/resources/pdfium/`. Use build **chromium/7881**, which matches `pdfium-render` 0.9.4. For example, on an Apple Silicon Mac:
  ```sh
  mkdir -p /tmp/pdfium && curl -sSL https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F7881/pdfium-mac-arm64.tgz | tar -xz -C /tmp/pdfium
  cp /tmp/pdfium/lib/libpdfium.dylib src-tauri/resources/pdfium/
  cp /tmp/pdfium/LICENSE src-tauri/resources/pdfium/LICENSE-pdfium
  ```
  > [!WARNING]
  > On macOS, if you replace `libpdfium.dylib`, delete the old file first. Copying over a library that's already loaded makes the next launch crash.
- **Ollama** with `llama3.1:8b` and `nomic-embed-text`, to try the AI features locally.

## Commands

| Command | What it does |
|---|---|
| `npm ci` | install JavaScript dependencies |
| `npm run desktop` | run the app in development, with the UI hot-reloading |
| `npm run desktop:build` | build an installer for this machine (in `target/release/bundle/`) |
| `npm run test:core` | run the core's unit tests |
| `npm run build` | build only the UI (`web/dist`) |
| `npm run notices` | regenerate `THIRD-PARTY-NOTICES.txt` (run it after changing dependencies) |

Release builds are slow (LanceDB is large) and use a lot of memory. On a 16 GB machine, close other heavy apps first.

### Environment variables

| Variable | Effect |
|---|---|
| `VAULTLM_DATA` | use a different app-data folder. Use it for testing, so you never touch your real data |
| `VAULT` | open this vault on start |
| `VAULTLM_LEGACY_DATA` | where to import the Node version's `data/` folder from on first launch. Set it to a missing path to skip the import |
| `VAULTLM_PDFIUM` | the folder containing the PDFium library (the bundled app sets this itself) |
| `VAULTLM_KEYS_IN_FILE` | store API keys in a test file instead of the keychain (tests only) |
| `CHAT_MODEL`, `EMBED_MODEL` | default Ollama models |
| `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` | fallback API keys |

## Testing

- **Core unit tests:** `npm run test:core`. These cover link resolution and rename rewriting, chunking, retrieval scopes, spaced repetition, PDF conversion, frontmatter round-trips and more.
- **UI against the real core:** the test harness `crates/vaultlm-core/examples/devserver.rs` serves `web/dist` and exposes the same API over HTTP, so a browser (for example Playwright with Chromium and WebKit) can drive the actual UI:
  ```sh
  npm run build
  VAULTLM_DATA=/tmp/vlm-data VAULT=/tmp/vlm-vault cargo run -p vaultlm-core --example devserver -- 3797
  # then open http://127.0.0.1:3797
  ```
  It's for development only and is never shipped.
- **Always test against copies.** Point `VAULTLM_DATA` and `VAULT` at throwaway folders (copy `examples/demo-vault`), never at your real notes or app data.

## Releases

`.github/workflows/release.yml` builds for macOS (Apple Silicon and Intel), Windows and Linux. It attaches the installers to a **draft** GitHub release named after the version in `src-tauri/tauri.conf.json`.

1. Bump `version` in `src-tauri/tauri.conf.json` and in `[workspace.package]` in the root `Cargo.toml`, and add an entry to `CHANGELOG.md`.
2. Push a tag (`git tag v0.2.0 && git push --tags`), or run the workflow by hand from the Actions tab.
3. When all four jobs are green, review the draft on the Releases page and publish it.

**Signing is optional.** With no secrets set, macOS builds are ad-hoc signed and Windows builds are unsigned, so users see a one-time warning. To sign, add these repository secrets; the workflow picks them up automatically:

| Secret | Used for |
|---|---|
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD` | the Developer ID Application certificate (`.p12`, base64-encoded) and its password |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | notarization (`APPLE_PASSWORD` is an app-specific password) |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | signing update bundles, once the auto-updater is enabled |

## Third-party notices

`scripts/third-party-notices.mjs` collects the license text of every library that ships in the app: the Rust runtime dependencies, plus the npm packages bundled into the UI. It writes them to `src-tauri/resources/THIRD-PARTY-NOTICES.txt`, which is bundled with the app and opened from **Help → Third-Party Licenses**. CI regenerates it on every release build; run `npm run notices` locally after changing dependencies and commit the result.

## The Node reference server

`server/` is the original implementation: a Fastify server that the same Vue UI talked to over HTTP. It's kept only as a reference while the desktop app is confirmed to cover everything, and will be removed after that. To run it:

```sh
npm run build && npm run server       # http://localhost:3737
VAULTLM_DATA=/tmp/vlm-node-data VAULT=/path/to/test-vault PORT=3799 npm run server   # isolated copy
```

## Logo and icons

The logo is `assets/logo.svg`. The app icons in `src-tauri/icons/` are generated from `assets/logo-1024.png` with `npx tauri icon assets/logo-1024.png`.
