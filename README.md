# Quire — Rust engines for Office documents

[English](README.md) · [繁體中文](README.zh-TW.md)

![Quire](brand/banner.png)

Quire is a Rust library for opening `.docx` files, laying them out, rendering the result
(one SVG per page / display list), and saving it back without corrupting anything: every
ZIP entry the engine did not touch is written back byte-for-byte. CI checks that claim on
a 399-file real-world corpus (`roundtrip --check`, byte-identical); the same gates build
for native and `wasm32-unknown-unknown` on Linux and Windows.

It is a library plus thin demo shells — not a word-processor product. DOCX works today;
XLSX is in progress as a sibling crate on the same container/lossless layer; PPTX comes
after ([roadmap](#roadmap) · [quireoffice.dev/roadmap](https://quireoffice.dev/roadmap)).

## What works today (DOCX)

- **Lossless open/save** — hand-rolled ZIP layer; untouched entries are copied
  byte-for-byte (ZIP64, extra fields, comments included). `roundtrip --check` diffs per
  file and gates CI.
- **Line breaking** — real font metrics (swash + a bundled Noto Sans TC subset),
  Knuth-Plass per-paragraph optimum, UAX#14 break rules; golden snapshots lock output.
- **Pagination** — paragraph caches + incremental reflow (incremental rebuilds are
  checked against full rebuilds; p95 < 8 ms); page geometry from `sectPr`, per page.
- **Content** — tables with cross-page slicing, headers/footers with field substitution,
  style cascade (docDefaults → basedOn → direct formatting), image placeholders and
  anchored floating images, forced page breaks (`w:br type="page"`).
- **Rendering** — one display-list contract: the CLI emits SVG, the browser shell (wasm)
  consumes the same output.
- **Comparison tooling** — `scripts/render_lab.py` diffs our render against LibreOffice
  page by page. Visual fidelity is still converging; complex drawing families are a
  known gap, tracked as corpus failure clusters.

Not built yet: editing UI, columns (`w:cols`), tracked changes, formula engine (XLSX),
PPTX, collaboration (the CRDT layer is currently an empty adapter slot).

## Quick start

    cargo test --workspace                        # engine tests
    cargo run -p quire-cli -- info fixtures/sample.docx
    cargo run -p quire-cli -- render fixtures/sample.docx --out out/quire-sample
                                                  # one SVG per page + break points
    cargo run -p quire-cli -- roundtrip corpus --check
                                                  # lossless gate over the corpus
    python3 scripts/render_lab.py --n 10          # LibreOffice baseline vs ours
    python3 scripts/verify_corpus.py              # corpus gate (count/manifest/features)

Browser demo: <https://quireoffice.dev/playground> — the drop zone is wired, the
in-browser render is not live yet; the page states what works and what doesn't.
Web shell from source: `cd shells/web && npm install && npm run demo`

## Repository layout

    core/ooxml         DOCX container + parts (lossless read/write, styles, tables)
    core/layout        line breaking + pagination (incremental, budgeted)
    core/render-trait  display-list contract
    core/crdt          collaboration adapter slot (Phase 4)
    core/engine-api    the single entry point the shells use
    shells/native      CLI head (info / render / roundtrip)
    shells/web         browser shell (wasm32-unknown-unknown)
    scripts/           corpus collector, LibreOffice render-lab, gate scripts

Two boundaries are enforced by a script rather than convention: `core/` never imports
`shells/`, and shells enter the core only through `engine-api`
(`bash scripts/check-layering.sh`). Every crate under `core/` must build for
`wasm32-unknown-unknown`. CI runs fmt / clippy / test / roundtrip / wasm on ubuntu and
Windows (a macOS job exists but is currently paused for runner quota); the matrix itself
lives on the docs site.

## Roadmap

|        |                                                                                              |
|--------|------------------------------------------------------------------------------------------------|
| Now    | DOCX: lossless round-trip + layout pipeline (this repo)                                      |
| Active | XLSX: `core/xlsx` on the same OPC/lossless layer — workbook model first, cells next          |
| Next   | DOCX depth (tracked changes, columns), ordered by corpus failure clusters; collaboration behind the CRDT slot |
| Later  | PPTX (`core/pptx`): reuses layout/shaping, owns a canvas model                                |

## Brand assets

Logo, lockups, banner, favicons and usage rules: [`brand/`](brand/).

## License

Source code: **Apache License 2.0** (`LICENSE-APACHE`). Exceptions: the Noto Sans TC
subsets under `core/layout/assets/` keep the upstream **SIL Open Font License 1.1**
(`OFL.txt` there); `corpus/` holds test corpora whose per-file licensing is tracked in
`corpus/manifest.json`.
