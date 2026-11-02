# Quire — Rust 的 Office 文件引擎：存檔時原樣搬運未修改的 part

[English](README.md) · [繁體中文](README.zh-TW.md)

![Quire](brand/banner.png)

Quire 是以 Rust 開發的文件引擎，用於開啟、排版、渲染與無損存檔 Office 文件。它不是包裝桌面辦公室軟體的外殼，也不是把檔案拆進內部物件模型後全數重寫的解析器：Quire 存檔時會重新封裝，但只有引擎被要求變更的 part 會被重寫；其餘 part——包括引擎不認識的自訂內容——皆以原始位元組原封不動放回新檔案（相同的壓縮流、ZIP64、extra field、註解與順序）。在 400 份真實文件語料上，未修改任何 part 的存檔往返達到 399 份檔案逐位元組相同（1 份因上游 CRC 損毀隔離），每次推播都由 CI 在 Linux 與 Windows 上重驗一遍。

多數開源文件函式庫把檔案解析成自己的模型、再從頭寫回，於是未建模的內容就無聲消失：自訂 XML part、數位簽章資料、廠牌特有的格式設定。如果產品需要處理並輸出文件，「存檔後遺漏部分原始內容」是無法接受的缺陷——因此未修改 part 的位元組級原樣保存是 Quire 最優先達成的保證，並由 CI 嚴格把關。Quire 是函式庫加上幾層很薄的展示殼：是你嵌進產品裡的引擎，不是安裝來用的文書處理器。

## 三分鐘看到它動

**在瀏覽器裡**——[quireoffice.dev/playground](https://quireoffice.dev/playground)：拖一份 `.docx` 進去。解壓、解析、排版、繪製全部在那個分頁內以 WebAssembly 完成，檔案不會離開你的裝置。目前是唯讀預覽，編輯是下一個引擎里程碑。

**從原始碼**

```bash
cargo test --workspace                          # 引擎測試
cargo run -p quire-cli -- render fixtures/sample.docx --out out/quire-sample
                                                # 每頁一張 SVG，含斷點報告
cargo run -p quire-cli -- roundtrip corpus --check
                                                # 無損閘門，399/399
```

## 做完了什麼、還沒做什麼

**已落地** — DOCX 無損開檔／存檔（含 ZIP64、extra field、註解）；Knuth-Plass 斷行搭配 UAX#14 與真實字型度量；增量分頁（500 頁文件 p95 < 8 ms）；表格跨頁切片並重發表頭；頁首頁尾與欄位代換；完整樣式級聯；多欄版面（`w:cols`）；修訂標記渲染（`w:ins`／`w:del`）；浮動圖片、表格底紋、圖片 duotone。

**進行中** — 文件編輯 API（下一個引擎里程碑）。XLSX 目前已共用同一層無損容器：`core/xlsx` 對 80 檔真實語料（POI 40＋LibreOffice QA 40）做容器級 byte-identical 往返，加上唯讀的 workbook 模型；cell 解析尚未實作。

**規劃中** — ODF 家族（ODT 先）；CRDT 協作層；瀏覽器編輯器、Tauri 桌面應用、可自架的協作後端；不列預估時程，細節見 [quireoffice.dev/roadmap](https://quireoffice.dev/roadmap)。

**已知缺口** — 對比 LibreOffice 的視覺再現仍在收斂，複雜繪圖族以語料失敗叢集追蹤。渲染是一份顯示列表（display list）契約，不是對任何辦公室軟體的像素級複製。

## 為開啟「你不信任的檔案」而設計

只會吃正常檔案的文件引擎等於展示品。Quire 把畸形輸入預設當成敵意處理：單一 entry 與整包的解壓資源上限（zip bomb 在膨脹前就被擋下）、每個解壓 part 都做 CRC-32 驗證、圖片像素預算並在 32 位元 wasm 上用 checked 算術、外加 fuzz-lite 回歸閘門斷言變異與截斷過的封包只能回報錯誤、不得 panic。解析 crate 的 release 設定保留 `overflow-checks`。細節見 [SECURITY.md](SECURITY.md)。

## 為什麼自己寫 ZIP 與排版層

現成的 ZIP 函式庫會重新壓縮資料流、重寫目錄記錄順序、丟掉不認識的 extra field——每一樣都讓逐位元組往返變成不可能，所以容器層自己寫。排版是同一件事的另一面：瀏覽器排版引擎每個平台結果不同，也無法在無介面環境跑，所以斷行與分頁是可決定的 Rust，在伺服器、CLI 與瀏覽器分頁裡產出同一份顯示列表。

兩條界線由腳本強制、不靠默契：`core/` 不得 import `shells/`，外殼只能經 `engine-api` 進入核心（`bash scripts/check-layering.sh`）。`core/` 下每個 crate 都要能編 `wasm32-unknown-unknown`。CI 在 Ubuntu 與 Windows 跑 fmt／clippy／test／roundtrip／wasm。

## 倉庫結構

    core/ooxml         DOCX 容器與 parts（無損讀寫、樣式、表格）
    core/xlsx          XLSX 活頁簿與工作表模型（共用同一層無損容器）
    core/layout        斷行＋分頁（增量、有預算）
    core/render-trait  顯示列表契約
    core/crdt          協作適配槽（規劃中）
    core/engine-api    外殼唯一進入點
    shells/native      CLI（info / render / roundtrip）
    shells/web         瀏覽器外殼（wasm32-unknown-unknown）
    scripts/           語料收集、LibreOffice 比對實驗室、閘門腳本

深度內容在文件站 [docs.quireoffice.dev](https://docs.quireoffice.dev)——架構筆記、CLI 參考、渲染實驗室流程、playground 說明。品牌素材（標誌、組合圖、banner、使用規範）在 [`brand/`](brand/)。

## 授權

原始碼採 **Apache License 2.0**（`LICENSE-APACHE`）。`core/layout/assets/` 下的 Noto Sans TC 字型子集保留上游 **SIL Open Font License 1.1**（該目錄 `OFL.txt`）；`corpus/` 為測試語料，逐檔授權追蹤於 `corpus/manifest.json`。
