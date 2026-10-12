# Vault-LM user guide

- [Vaults, notebooks, sections and topics](#vaults-notebooks-sections-and-topics)
- [Chat](#chat)
- [Studio: study tools](#studio-study-tools)
- [Review dashboard](#review-dashboard)
- [The editor](#the-editor)
- [Organising](#organising)
- [Sources](#sources)
- [Models](#models)
- [Where things are stored](#where-things-are-stored)
- [Keyboard shortcuts](#keyboard-shortcuts)

## Vaults, notebooks, sections and topics

A **vault** is any folder of Markdown notes. An Obsidian vault works as-is. Inside it:

| Term | What it is |
|---|---|
| **Notebook** | a folder under the vault's notebook root. The root is the vault itself by default; change it with ⚙ next to *Notebooks*, e.g. to `Efforts/` |
| **Section** | a subfolder of a notebook |
| **Topic** | a single `.md` note |

- Open or create vaults with **File → Open Vault… / New Vault…**. Switch between them with the vault picker at the top of the sidebar.
- The **Notebooks tree sets the scope.** Whatever you select there (a notebook, section or topic) is what chat and Studio work on.
- Opening a note from *Vault files*, search or a citation only opens a tab. It doesn't change your selection.
- Each notebook remembers its own tabs, selection, chat, mode and Studio state.

### Working alongside Obsidian

- Vault-LM never writes to `.obsidian/`. It only reads your attachment-folder setting so pasted images land in the same place.
- It uses standard `[[wikilinks]]` and YAML frontmatter.
- Deleted notes go to the vault's `.trash/` folder, not straight to oblivion.

## Chat

Ask questions about the selected notebook. Answers cite the notes they come from: click a citation chip to open that note at the right place.

Chat never uses anything outside the current notebook. There are two modes:

| Mode | What the model sees |
|---|---|
| **Reference** (default) | your selection first, then the most relevant passages from the rest of the notebook, labelled by section |
| **Focused** | only your selection, plus the auto-generated **Notebook Overview** (`<notebook>/_studio/Notebook Overview.md`) for background |

- **Several chats per notebook.** Start one with **+**; titles are generated automatically.
- **Save as topic** on any answer saves it as a new note, or appends it to an existing one.
- **Follow-up suggestions** appear under each answer.
- **Search behind the scenes** combines keyword matching with meaning-based search, so exact names and numbers are always found as well as related ideas.

## Studio: study tools

Open Studio with the panel button at the top right (⌘.). Every tool can target the notebook, the section, or the topic you have selected.

| Tool | What it does |
|---|---|
| **Flashcards** | generates a deck and schedules reviews with spaced repetition (SM-2). Also writes a `#flashcards` note that the Obsidian Spaced Repetition plugin can read |
| **Quiz** | multiple-choice questions with explanations. Missed questions feed your weak spots |
| **Mind map** | an interactive map of the material's structure |
| **Explain back** | you explain the topic in your own words; the model grades it and points out gaps (the Feynman technique) |
| **Briefs** | a summary, FAQ or study guide |
| **Audio overview** | a two-host conversation about the material, read aloud with your system's voices |

Everything Studio generates is saved in `<notebook>/_studio/`. That folder is never indexed, so generated material doesn't leak back into chat answers.

### Details and goals

Use ⋯ → **Edit details…** on a notebook or section to add a description, goals, status, due date and colour.

- They're stored in the app's own data folder, never in your vault.
- They steer chat answers, the Notebook Overview, and every Studio tool. For example, "I'm studying for the CAPM exam in June" makes quizzes focus on exam-style questions.

## Review dashboard

Open it from **Review** in the sidebar (⇧⌘R).

- **Cards due** across every notebook, with one mixed review session.
- **Weak spots** from missed quiz questions, cards you marked "Again", and gaps found by Explain back. Recent attempts count more. **Drill weak spots** builds a session from them.
- An **activity heatmap**, quiz accuracy, and upcoming due dates.

## The editor

- **Live preview.** Markdown renders inline, except on the line you're editing.
  - It renders `[[links]]` as chips, images, checkboxes, callouts, a Properties panel and `#tags`.
  - Type `[[` to autocomplete notes and headings, or `#` for tags.
  - It saves automatically, and warns you if the file was changed outside the app.
  - Pasting or dropping an image saves it as an attachment.
- **Select text** to Explain, Simplify (and replace), Make a flashcard, Quiz yourself, or Ask in chat.
- **Side panel:**
  - properties, tags and outline
  - links and backlinks
  - **unlinked mentions**, with one-click *Link*
  - **related topics** by meaning
  - **Suggest links** from the AI
- **Graph:** use the graph button in the header for a map of the notebook. Nodes are coloured by section, with an optional layer of "similar notes" connections.
- **Search (⌘K)** looks through note names, headings, full text and `#tags`, and offers "Ask in chat" if nothing matches.

## Organising

- **Rename** a topic, section or notebook and every `[[link]]` to it across the vault is updated. A topic's sources move with it.
- **Move** topics and sections by dragging them in the tree, or with ⋯ → *Move to…*. Undo is available. Links, sources, details and flashcard decks follow.
- **Templates:** create them with ⚙ → *Manage topic templates…*, then use them from the ▾ next to *New topic*. Available placeholders: `{{title}} {{date}} {{time}} {{notebook}} {{section}}`.

## Sources

Add outside material with the paperclip on a notebook, section or topic. Each source is converted to Markdown and saved next to what it belongs to, so you can read and search it like a note:

| Added to | Saved in |
|---|---|
| a notebook | `<notebook>/sources/` |
| a section | `<section>/sources/` |
| a topic | `<folder>/sources/<Topic>/` (your note itself is never edited) |

Supported:
- **Web pages** (the article is extracted, without menus and ads).
- **PDFs**, converted with layout awareness: headings, lists and tables are kept. Scanned PDFs are read with on-device OCR.
- **EPUB** books, **images** (OCR), and `.txt` / `.md` / `.html`.
- **YouTube transcripts.** This is *experimental*: it relies on YouTube's unofficial API, so it can stop working without warning.

The OCR model downloads once, the first time you need it (about 12 MB). Sources can be renamed, reprocessed (for example after an app update improves PDF conversion) or removed.

## Models

Click the model name under the chat box, or use **Models…** (⌘,):

- **Local · Ollama:** lists your installed models with size and quantization. **Download** pulls new ones, with progress shown in the corner.
- **Claude · Anthropic:** paste an API key, then pick from the live model list. *Effort* can be low, medium or high.
- **OpenAI-compatible:** works with OpenAI, OpenRouter, Groq, LM Studio, llama.cpp, vLLM and similar. Set a base URL and an optional key.
- **Search embeddings:** Ollama or OpenAI-compatible (Anthropic has no embeddings API). Each embedding model gets its own index, so switching rebuilds it once and switching back is instant.

API keys are stored in your system keychain (macOS Keychain, Windows Credential Manager, or the Secret Service on Linux). The environment variables `ANTHROPIC_API_KEY` and `OPENAI_API_KEY` also work.

With a cloud model, your question and the matching note excerpts go to that provider, and the app shows ☁ next to the model name.

## Where things are stored

| What | Where |
|---|---|
| Your notes, sources and Studio output | your vault folder |
| Search index, flashcard schedules, progress, chats, details, settings | the app data folder (below) |
| API keys | the system keychain |

The app data folder is:
- **macOS:** `~/Library/Application Support/com.vaultlm.Vault-LM`
- **Windows:** `%APPDATA%\vaultlm\Vault-LM\data`
- **Linux:** `~/.local/share/vault-lm`

**Help → Show App Data Folder** opens it. Deleting it resets the app; your notes aren't affected.

## Keyboard shortcuts

Shortcuts are shown for macOS; on Windows and Linux use Ctrl instead of ⌘.

| Shortcut | Action |
|---|---|
| ⌘K | Search |
| ⌘O | Open vault |
| ⇧⌘N | New vault |
| ⌘, | Models |
| ⌘. | Toggle Studio |
| ⇧⌘R | Review dashboard |
| ⇧⌘D | Toggle dark mode |
| ⌘W | Close tab |
| ⇧⌘W | Close window |
