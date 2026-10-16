# Quire — lossless Office document engines in Rust

[English](README.md) · [繁體中文](README.zh-TW.md)

![Quire](brand/banner.png)

**Ships today: DOCX.** XLSX and PPTX join as sibling format crates on the same core
(see [Roadmap](#roadmap)).

**Lossless by construction.** Parts Quire doesn't touch are written back byte-for-byte
(130/130 real-world corpus files round-trip byte-identical in CI — ZIP64 and unknown
extensions included), while the layout pipeline — Knuth-Plass optimal line breaking,
UAX#14 break rules, incremental pagination, real font metrics — produces display lists
that both a native and a browser renderer consume.

## Layering (enforced in CI)

The typesetting engine is a single Rust core compiled to two targets (native and
`wasm32-unknown-unknown`); the shells — a desktop CLI head (`shells/native`) and a
browser shell (`shells/web`) — are thin platform adapters. The boundary is
machine-enforced, not convention:

    shells/native ─┐                 ┌─ core/ooxml        (DOCX zip+XML, lossless part retention)
                   ├─ engine-api ────┼─ core/layout       (line breaking + pagination; incremental/budgeted reflow)
    shells/web ────┘   (sole entry)   ├─ core/render-trait (display-list contract)
                                     └─ core/crdt         (loro adapter slot, Phase 4)

- `bash scripts/check-layering.sh` — core never imports shells; shells go through engine-api only
- CI rule: every crate in `core/` must build for `wasm32-unknown-unknown` (dual-target is the premise of building both together)

### CI matrix (three OSes × two targets)

The `cross-platform` job in `.github/workflows/ci.yml` runs the same gates on ubuntu / macOS / Windows runners. `ci-green` is the single aggregate check (bind branch protection to it alone, so adding or removing OSes later needs no reconfiguration):

| Gate | ubuntu | macOS | Windows |
|---|---|---|---|
| `cargo fmt --check` | ✅ baseline | — (formatting must not vary by OS) | — |
| `cargo clippy -D warnings` (host target) | ✅ | ✅ arm64 + amd64 | ✅ |
| `cargo test --workspace` | ✅ | ✅ | ✅ |
| ★ lossless round-trip (fixtures + 130-file corpus) | ✅ | ✅ | ✅ |
| ★ core + web shell compile for `wasm32-unknown-unknown` | ✅ | ✅ | ✅ |
| wasm shell actually links a `.wasm` | ✅ | ✅ | ✅ |
| layering check | ✅ | ✅ | ✅ |

`macos-latest` is Apple Silicon (host = `aarch64-apple-darwin`), so that platform adds an extra `x86_64-apple-darwin` clippy cell — both amd64 and arm64 are covered.

`.gitattributes` pins text files to LF and `.docx` to binary. Without it, `core.autocrlf=true` on Windows runners makes `cargo fmt --check` fail spuriously, and git's newline normalization would break the byte-level round-trip assertions.

## Quick start

    cargo test --workspace                    # engine tests (includes a real docx fixture)
    cargo run -p quire-cli -- info fixtures/sample.docx
    cargo run -p quire-cli -- render fixtures/sample.docx --out out/render-lab/quire-sample   # one SVG per page + break points
    python3 scripts/render_lab.py --n 10        # LibreOffice baseline vs ours: diff report over first 10 corpus files
    python3 scripts/verify_corpus.py          # corpus gate (≥100 files / manifest / feature coverage)
    python3 scripts/corpus_parse_smoke.py     # corpus green-light rate (currently 130/130)
    cargo run -p quire-cli -- roundtrip <file|dir> --check   # lossless round-trip gate
    cd shells/web && npm install && npm run demo             # browser rendering demo
    cd shells/web && npm run build:wasm && npm run build:collab && npm run size   # bundle split + size gate

## Status (skeleton)

| Item | Status |
|---|---|
| DOCX parsing (paragraphs/runs/bold-italic/styles/entities/`&`, lossless part retention) | ✅ skeleton |
| Lossless round-trip (hand-rolled zip: untouched entries copied byte-for-byte + ZIP64 + extra/comment) | ✅ |
| `roundtrip --check` comparer (structural level, allow-list of permitted diffs) + CI gate | ✅ |
| Line breaking (Latin whitespace + CJK anywhere, width-constrained), pagination | ✅ skeleton (greedy) |
| Display-list JSON contract + CLI render head | ✅ skeleton |
| render-lab (headless docx→SVG + LibreOffice baseline diff report) | ✅ skeleton `scripts/render_lab.py` |
| wasm32 dual-target CI gate (3 OS × amd64/arm64 + wasm32 matrix) | ✅ |
| wasm bundle split + first-payload size gate (CRDT deferred chunking; first bundle gzip ≈ 54 KB) | ✅ `scripts/check-wasm-size.mjs` |
| Real DOCX corpus (130 files + manifest + feature coverage) | ✅ `scripts/corpus/collect.sh` |
| Real font-metric shaping (swash + bundled Noto Sans TC subset) + golden break-point snapshots | ✅ `core/layout/src/shaping.rs` |
| Page geometry from `sectPr` (pgSz/pgMar/type; Letter/landscape/multi-section reported per page) | ✅ `core/ooxml/src/sectpr.rs` |
| Knuth-Plass optimal line breaking + UAX#14 break rules | ✅ `core/layout/src/kp.rs` |
| Paragraph-level cache + incremental pagination + budgeted soft-paginate/background reflow (incremental ≡ full rebuild, p95 < 8 ms) | ✅ `core/layout/src/lib.rs` + `examples/p6_bench.rs` |
| Tables, headers/footers, columns, tracked changes | ⏳ Phase 3–4 (driven by corpus green-light rate) |

## Roadmap

Quire is an Office-format engine family, not a single-format project. The bet is that
DOCX/XLSX/PPTX share most of what is hard — the OPC container (zip+XML, lossless at
byte level), OOXML part/model handling, and the same lossless discipline — so each new
format reuses that core instead of forking it, while keeping the layout model its
format demands (flow for DOCX, grid for XLSX, canvas for PPTX):

| Milestone | Scope |
|---|---|
| **Now** | DOCX: lossless round-trip + layout/pagination skeleton (this repo) |
| **Phase 3–4** | DOCX depth: tables, headers/footers, columns, tracked changes — driven by corpus green-light rate; CRDT collaborative layer (loro adapter slot) |
| **Then: XLSX** | `core/xlsx` sibling format crate on the same OPC/lossless core: workbook model, formula engine, sheet render list (charts later) |
| **Then: PPTX** | `core/pptx`: slide/shape model reusing layout + shaping; presentation render list |
| **Shell split** | Shells stay in-tree until a shell ships releases of its own (installers / deployed app), its license diverges, or contributor communities diverge — the layering gate is what keeps that split cheap |

## Brand assets

Logo, lockups, banner, favicons and usage rules live in [`brand/`](brand/).

## License

Source code is released under the **Apache License 2.0**; see `LICENSE-APACHE`.

Exceptions: the Noto Sans TC subset fonts under `core/layout/assets/` remain under their upstream **SIL Open Font License 1.1** (see `OFL.txt` in that directory); `corpus/` holds test corpora whose per-file licensing is tracked in `corpus/manifest.json`.
