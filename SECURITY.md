# Security Policy

## Supported versions

| Version | Supported |
|---|---|
| latest `v*` release tag | ✅ |
| `main` | ✅ (pre-release; everything on `main` ships to the playground within one release cycle) |

## Reporting a vulnerability

**Do not open a public issue for security problems.** Use GitHub
[private vulnerability reporting](https://github.com/quire-office/quire/security/advisories/new)
on this repository. We aim to acknowledge within 3 business days and ship a fix
before public disclosure whenever feasible.

## Threat model (what counts as a security problem here)

The engine is designed to process **untrusted documents**: the hosted playground
opens user-uploaded `.docx` files inside the browser tab (wasm), and the CLI is
routinely pointed at files from the wild. Therefore:

- **Resource limits are part of the security contract.** Decompression is
  bounded per entry and per archive; PNG decoding is bounded by a pixel budget
  with checked arithmetic on every target, including `wasm32` (32-bit `usize`).
  Exceeding a limit must produce a graceful error
  (`ZipError::TooLarge` / `decode_png_rgba8 → None`), never memory exhaustion.
  **Any input that bypasses these limits is a security issue** — please report it.
- **Parsers must never panic on malformed input.** `ZipBundle::open`,
  `OoxmlDoc::parse` and the PNG path may return `Err`/`None` on anything, but a
  panic — integer overflow abort, slice out-of-bounds, unwrap on attacker-controlled
  data — is a security issue. The regression gate is
  `core/ooxml/tests/s3_malformed_smoke.rs` (deterministic mutation fuzzing).
- **Lossless round-trip is an integrity guarantee.** Silent byte corruption of
  untouched parts (beyond the documented quarantine list) is treated as a
  security-relevant defect, not just a bug.

## Scope notes

Rendering fidelity differences vs. LibreOffice are **not** security issues.
Fonts under `core/layout/assets/` and corpus files keep their upstream licenses
and are out of scope.
