# Quire — Rust 的 Office 文件引擎

[English](README.md) · [繁體中文](README.zh-TW.md)

![Quire](brand/banner.png)

Quire 是一個 Rust 函式庫：開啟 `.docx`、排版、輸出渲染結果（每頁一張 SVG／顯示
列表），存回時不動的東西一個位元組都不會變——引擎未觸碰的 ZIP entry 原樣寫回。
這個說法由 CI 在 399 份真實語料上逐檔檢查（`roundtrip --check`，byte-identical），
native 與 `wasm32-unknown-unknown` 兩個目標在 Linux 與 Windows 上都跑。

它是函式庫加一個很薄的展示殼，不是成品文書軟體。DOCX 現在可用；XLSX 以平級
crate 進行中，掛在同一層容器／無損機制上；PPTX 排在其後
（[後續規劃](#後續規劃) · [quireoffice.dev/roadmap](https://quireoffice.dev/roadmap)）。

## 現在能用的（DOCX）

- **無損開／存**——手寫 ZIP 層，未觸碰的 entry 位元組級原樣搬運（含 ZIP64、
  extra field、comment）。`roundtrip --check` 逐檔比對並進 CI 閘門。
- **斷行**——真實字型度量（swash＋內建 Noto Sans TC 子集）、Knuth-Plass 逐段
  最佳化、UAX#14 斷字規則；輸出由 golden 快照鎖住。
- **分頁**——段落級快取＋增量重排（增量結果與整篇重建逐頁核對；p95 < 8 ms）；
  頁面幾何取自 `sectPr`，逐頁回報。
- **內容**——表格（含跨頁切頁）、頁首頁尾（含欄位代換）、樣式級聯
  （docDefaults → basedOn → 直接格式）、圖片佔位框與錨定浮圖、強制分頁
  （`w:br type="page"`）。
- **渲染**——同一份顯示列表契約：CLI 輸出 SVG，瀏覽器殼（wasm）吃同樣的輸出。
- **比對工具**——`scripts/render_lab.py` 拿 LibreOffice 做基線逐頁比對渲染差異。
  視覺還在對齊中；複雜圖形族是已知的缺口，以 corpus 失敗簇追蹤。

還沒做的：編輯 UI、欄位排版（`w:cols`）、修訂追蹤、公式引擎（XLSX）、PPTX、
協作（CRDT 層目前是空的適配位）。

## 快速開始

    cargo test --workspace                        # 引擎測試
    cargo run -p quire-cli -- info fixtures/sample.docx
    cargo run -p quire-cli -- render fixtures/sample.docx --out out/quire-sample
                                                  # 每頁一張 SVG＋分頁斷點
    cargo run -p quire-cli -- roundtrip corpus --check
                                                  # 語料無損閘門
    python3 scripts/render_lab.py --n 10          # LO 基線 vs 我方
    python3 scripts/verify_corpus.py              # 語料閘門（檔數/manifest/覆蓋率）

瀏覽器 demo：<https://quireoffice.dev/playground>——拖放區已就緒，瀏覽器端渲染尚未上線，
該頁會說明哪些能用、哪些還沒。
本機跑網頁殼：`cd shells/web && npm install && npm run demo`

## 倉庫結構

    core/ooxml         DOCX 容器＋part（無損讀寫、樣式、表格）
    core/layout        斷行＋分頁（增量、預算制）
    core/render-trait  顯示列表契約
    core/crdt          協作適配位（Phase 4）
    core/engine-api    殼進核心的唯一入口
    shells/native      CLI（info / render / roundtrip）
    shells/web         瀏覽器殼（wasm32-unknown-unknown）
    scripts/           語料採集、LibreOffice render-lab、閘門腳本

兩條界線由腳本檢查、不靠 review 約定：`core/` 不得 import `shells/`，殼只能走
`engine-api`（`bash scripts/check-layering.sh`）。`core/` 下每個 crate 都必須能編
`wasm32-unknown-unknown`。CI 在 ubuntu、Windows 跑
fmt／clippy／test／roundtrip／wasm；矩陣細節在文件站。

## 後續規劃

|      |                                                                                          |
|------|--------------------------------------------------------------------------------------------|
| 現在 | DOCX：無損 round-trip ＋ 排版管線（本 repo）                                              |
| 進行中 | XLSX：`core/xlsx` 掛同一 OPC／無損層——先活頁簿模型，接著儲存格                            |
| 接著 | DOCX 深度（修訂追蹤、欄位排版），順序由 corpus 失敗簇決定；協作走 CRDT 適配位              |
| 之後 | PPTX（`core/pptx`）：複用 layout／shaping，自帶畫布模型                                    |

## 品牌資產

標誌、lockup、banner、favicon 與使用規則見 [`brand/`](brand/)。

## 授權

原始碼以 **Apache License 2.0** 授權發布（`LICENSE-APACHE`）。例外：
`core/layout/assets/` 下的 Noto Sans TC 子集字型依上游 **SIL Open Font License 1.1**
授權（見同目錄 `OFL.txt`）；`corpus/` 為測試語料，各檔授權追蹤於
`corpus/manifest.json`。
