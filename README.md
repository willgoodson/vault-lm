# Vault-LM

## Desktop app (native Rust, Tauri v2)

The desktop app is the main way to run Vault-LM. It's a native Rust core (`crates/vaultlm-core`) and a Tauri shell (`src-tauri/`) that reuses the Vue UI in `web/` over IPC, with no local server and no open ports.

| | |
|---|---|
| Run in development | `npm run desktop` |
| Build the app | `npm run desktop:build` (unsigned locally; CI signs and notarizes) |
| Core tests | `npm run test:core` |
| Build prerequisites | Rust, Node 22, `protoc` (`brew install protobuf`), PDFium in `src-tauri/resources/pdfium/` (CI downloads it) |

- **Data:** app data lives in the OS app-data folder (`~/Library/Application Support/com.vaultlm.Vault-LM` on macOS). On first launch, your data from the Node version (`data/`) is imported: indexes are reused without re-embedding, and decks, details, progress and chats come along. API keys are stored in the **OS keychain**.
- **Engines:**
  - LanceDB (the same index format as before, plus BM25 full-text search)
  - PDFium (layout-aware PDF conversion that detects real bold text)
  - ocrs (on-device OCR; its models download once, about 12 MB)
  - the OS's own voices for audio overviews (macOS `say`, Windows System.Speech, Linux espeak-ng)
  - models via Ollama, Anthropic (raw HTTP), or any OpenAI-compatible server
- **Releases:** `.github/workflows/release.yml` builds macOS (arm64 and x64), Windows and Linux installers on tag push, once the signing secrets are set.
- The logo is `assets/logo.svg`. The icons in `src-tauri/icons/` are generated from it with `npx tauri icon assets/logo-1024.png`.

The Node server below is the original implementation. It stays as a reference until you've confirmed the desktop app covers everything you use.


A standalone, local NotebookLM plus a "second brain" app for Markdown vaults. It runs on Ollama (`llama3.1:8b` and `nomic-embed-text`), LanceDB, CodeMirror 6 and kokoro-js. Nothing leaves your machine.

Vaults stay **plain folders that Obsidian can open**. Vault-LM never writes to `.obsidian/` (it only reads the attachment setting), uses standard `[[wikilinks]]` and YAML frontmatter, and deletes into `.trash/`. It doesn't depend on Obsidian.

## Structure
- **Notebook**: a folder under the vault's notebook root (⚙ next to *Notebooks*; defaults to the vault root, or set it to e.g. `Efforts/`).
- **Section**: a subfolder of a notebook.
- **Topic**: a single `.md` note.
- **Sources**: imported URLs and PDFs, stored next to whatever they belong to:
  - `<notebook>/sources/`
  - `<section>/sources/`
  - `<dir>/sources/<Topic>/` for a topic. Your note itself is never edited.

The **Notebooks tree sets the scope.** Browsing *Vault files*, search or citations only opens tabs and never changes the selection. Each notebook keeps its own tabs, selection, chat, mode and Studio state. These are saved per vault in `data/vaults/<slug>/ui-state.json`.

## Chat modes (always within the notebook)
| Mode | Context |
|---|---|
| **Reference** (default) | your selection first, then the most relevant parts of the rest of the notebook, labeled by section |
| **Focused** | only your selection, plus the auto-generated **Notebook Overview** (`<notebook>/_studio/Notebook Overview.md`) for background |

## Studio
Flashcards (SM-2 spaced repetition, plus a Spaced Repetition-plugin `#flashcards` mirror), quizzes, mind maps, explain-back (Feynman) grading, briefs and audio overviews.
- Each tool can target the notebook, the section or the topic. They're labeled by name.
- Outputs go to `_studio/`, which is never indexed.

## Editor and second-brain features
- **Live-preview editor** (CodeMirror 6). Markdown renders inline except on the line you're editing.
  - Renders: `[[links]]` as chips, images, checkboxes, callouts, a Properties panel, `#tags`.
  - `[[` autocomplete for notes and headings, `#` for tags.
  - Autosaves, and detects conflicting edits made outside the app.
  - Pasting or dropping an image saves it as an attachment.
  - Enter on an empty list item ends the list.
- **Rename** a topic, section or notebook and `[[links]]` across the vault update. A topic's sidecar sources move with it.
- **Side panel:** properties, tags, outline, links, backlinks, **unlinked mentions** (one-click "Link"), **related topics** by meaning, and **Suggest links** from the AI.
- **Graph** tab for each notebook. Nodes are colored by section, with an optional layer of "similar notes" edges.
- **Select text** in the editor to Explain, Simplify (with replace), Make a flashcard, Quiz yourself, or Ask in chat.
- **⌘K** searches note names, headings, full text and `#tags`, with an "Ask in chat" fallback.

## Studying and organizing (v4)
- **Details and goals** (⋯ → *Edit details…* on a notebook or section) cover description, goals, status, due date and color. They're stored in Vault-LM's `data/` folder, never in the vault. They steer chat answers, the notebook overview, and every Studio tool.
- **Review dashboard** (sidebar → *Review*):
  - cards due across every notebook, with one mixed review session
  - **weak spots** from missed quiz questions, "Again" cards and explain-back gaps, weighted toward recent attempts, with **Drill weak spots**
  - an activity heatmap, quiz accuracy, and upcoming due dates
- **Chats:** several named chats per notebook (titled automatically), and **Save as topic** on any answer, either as a new note or appended to an existing one.
- **Hybrid search:** BM25 keyword search plus vectors, fused with reciprocal-rank fusion, so exact names, numbers and terms are always found. ⌘K uses the same index.
- **Drag-and-drop** topics and sections in the tree, or use ⋯ → *Move to…*, with Undo. Links, sidecar sources, details and decks follow.
- **Topic templates** (⚙ → *Manage topic templates…*, or the ▾ next to New topic) with `{{title}} {{date}} {{time}} {{notebook}} {{section}}`.
- **Sources:** web pages, **YouTube transcripts**, PDFs (scans are OCR'd locally), **EPUB**, **images** (OCR), and .txt/.md/.html. The OCR language model downloads once to `data/tesseract/`.

## Models
Click the model name under the chat box (or ⚙ → *Models…*) to choose:
- **Local · Ollama:** the installed models, with size and quantization, and **Download** to pull new ones (progress shows in the corner).
- **Claude · Anthropic:** paste an API key, then pick from the live model list. Effort can be low, medium or high. Supported models opt into Anthropic's server-side refusal fallback.
- **OpenAI-compatible:** OpenAI, OpenRouter, Groq, LM Studio, llama.cpp, vLLM and so on. Set a base URL and an optional key.
- **Search embeddings:** Ollama or OpenAI-compatible (Anthropic has no embeddings API). Each embedding model gets its own index, so switching rebuilds it once and switching back is instant.

Settings and keys are stored in `data/models.json` (file mode 600) and are never sent to the browser. `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` work as fallbacks. With a cloud model, your question and the retrieved note excerpts go to that provider, and the app shows ☁ next to the model name.

## Run
```sh
ollama serve            # if not already running
npm run build           # build the UI (after UI changes)
npm run server          # http://localhost:3737
npm run dev             # hot-reload UI on :5173 + server
npm run index           # manual reindex
```
Override the models with `CHAT_MODEL` / `EMBED_MODEL`, and the starting vault with `VAULT=/path`.

## License
MIT. See [LICENSE](LICENSE).
