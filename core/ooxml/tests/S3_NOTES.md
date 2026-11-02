# S3 fuzz-lite：ZIP / OOXML / PNG 解析器「永不 panic」突變閘門

外部 code review（2026-11-02，用戶交付兩份）指出：399 檔語料只覆蓋合法文件，
ZIP/PNG 解析是主要攻擊面而 repo 無 fuzz 設施。本檔是 cargo-fuzz 的 CI 可跑替代：
決定性突變（線性同餘 PRNG、固定 seed）＋`catch_unwind` 斷言任何輸入都只回
`Err`/`None`、**絕不 panic**。突變空間小而穩定（CI 零不確定性）；日後接
cargo-fuzz 時本檔保留作為回歸鎖。

跑法：`cargo test -p quire-ooxml --test s3_malformed_smoke`
巨型 PNG 案例預設跳過（未加固代碼上會 OOM 而非 panic，見檔內說明）。
