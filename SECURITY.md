# Security policy

## Reporting a vulnerability

Please **don't open a public issue** for security problems. Instead, report them privately through GitHub: go to the repository's **Security** tab and choose **Report a vulnerability**.

Include what you found, how to reproduce it, and what an attacker could do with it. I'll reply as soon as I can and keep you updated until it's fixed.

## Supported versions

Security fixes go into the latest release.

## Scope

Vault-LM is a local desktop app with no server component. The areas that matter most:
- **Files:** anything that reads or writes outside the open vault or the app's data folder.
- **API keys:** anything that exposes keys stored in the system keychain.
- **Imported sources:** web pages, PDFs, EPUBs and other content imported as sources, which is untrusted input.
- **The UI:** script injection through note content or imported material.
