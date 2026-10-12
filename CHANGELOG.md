# Changelog

All notable changes to Vault-LM are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed
- The Fedora/RHEL `.rpm` now installs `espeak-ng` (used for audio overviews), like the `.deb` already did.

## [0.1.0] - 2026-10-12

The first release of the desktop app for macOS, Windows and Linux.

### Added
- Notebooks, sections and topics on top of any folder of Markdown notes, including Obsidian vaults.
- Chat with citations, in Reference and Focused modes, with several named chats per notebook and **Save as topic**.
- Studio: flashcards with spaced repetition, quizzes, mind maps, Explain back, briefs and audio overviews.
- Review dashboard with due cards, weak spots, an activity heatmap and **Drill weak spots**.
- Live-preview Markdown editor with wikilinks, backlinks, unlinked mentions, related topics and AI link suggestions.
- Graph view and ⌘K search (keyword + meaning-based).
- Sources: web pages, PDFs (layout-aware, with on-device OCR for scans), EPUB, images, text files, and YouTube transcripts (experimental).
- Models: Ollama, Anthropic (Claude) and OpenAI-compatible providers, with API keys stored in the system keychain.
- Details and goals for notebooks and sections, topic templates, drag-and-drop moving with link updates.
- Native menus, a first-run screen, and a one-time import of data from the earlier Node version.

[0.1.0]: https://github.com/willgoodson/vault-lm/releases/tag/v0.1.0
