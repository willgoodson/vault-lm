# Contributing to Vault-LM

Thanks for your interest! Bug reports, ideas and pull requests are all welcome.

## Reporting bugs and suggesting features

[Open an issue](https://github.com/willgoodson/vault-lm/issues/new/choose) and pick the bug report or feature request template. For bugs, the most useful details are:
- your operating system
- the app version (shown in **About Vault-LM**)
- the model you were using
- the steps that cause the problem

Please don't include private notes or API keys in an issue.

## Working on the code

1. Follow [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) to set up Rust, Node, `protoc` and PDFium.
2. Run the app with `npm run desktop`.
3. For bigger changes, open an issue first so we can agree on the approach before you spend time on it.

Before opening a pull request:

- Run `npm run test:core`, and add tests to `crates/vaultlm-core` for new logic.
- Run `npm run build` to check that the UI still builds.
- Test against a copy of [`examples/demo-vault`](examples/demo-vault) with `VAULTLM_DATA` pointing at a scratch folder, not your own notes.
- If you add or upgrade a dependency, run `npm run notices` and commit the updated `THIRD-PARTY-NOTICES.txt`.
- Match the style of the surrounding code.

### Ground rules for changes

- **The vault must stay a plain folder that Obsidian can open.** Never write to `.obsidian/`, keep standard `[[wikilinks]]` and YAML frontmatter, and delete into `.trash/`.
- **Data about notes (details, progress, indexes) lives in the app data folder**, not in the vault.
- **Nothing leaves the user's machine** unless they've chosen a cloud model, and API keys stay in the system keychain.

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
