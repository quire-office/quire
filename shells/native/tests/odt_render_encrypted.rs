//! ODF-E1 CLI 考卷：`quire-cli odt-render` 對加密 ODT 必須非零退出＋老實報錯。
//!
//! 缺陷口徑（ODF-E1 卡）：加密檔（`corpus-odt/ext/…sha1_correct.odt`／
//! `…sha1_wrong.odt`，manifest 宣告 Blowfish CFB＋SHA1/1K）容器層開得了、
//! content.xml 實為密文——舊口徑投影成 `{"paragraph_count":0,...}` 且退出碼 0，
//! 空文件與「不支援的加密文件」在口徑上不可區分。
//!
//! 本考卷鎖 CLI 面三件事：
//! 1. 退出碼非零（不吞錯紀律）。
//! 2. stderr 含「加密不支援」＋演算法名 `Blowfish CFB`（失敗分群可辨識）。
//! 3. stdout 不得再出現零段 JSON（不得以正常結果出口）。
//!
//! 對照組：明文語料檔必須照常退出 0＋輸出含 paragraph_count（防過度拒收）。

use std::process::Command;

fn run_odt_render(rel: &str) -> std::process::Output {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus-odt")
        .join(rel);
    Command::new(env!("CARGO_BIN_EXE_quire-cli"))
        .arg("odt-render")
        .arg(&path)
        .output()
        .expect("執行 quire-cli odt-render")
}

fn assert_encrypted_cli(rel: &str) {
    let out = run_odt_render(rel);
    assert!(
        !out.status.success(),
        "{rel}：加密檔必須非零退出，實際退出碼 {:?}",
        out.status.code()
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("加密不支援"),
        "{rel}：stderr 應含「加密不支援」，實際: {stderr:?}"
    );
    assert!(
        stderr.contains("Blowfish CFB"),
        "{rel}：stderr 應含 manifest 宣告的演算法名 Blowfish CFB，實際: {stderr:?}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("paragraph_count"),
        "{rel}：不得再以零段 JSON 正常出口，實際 stdout: {stdout:?}"
    );
}

#[test]
fn odt_render_encrypted_sha1_pair_exits_nonzero_with_algorithm() {
    for rel in [
        "ext/lo_core_sw_qa_odfexport_sha1_correct.odt",
        "ext/lo_core_sw_qa_odfexport_sha1_wrong.odt",
    ] {
        assert_encrypted_cli(rel);
    }
    // 對照組：明文檔照常成功（加密閘門不得誤傷正常投影路徑）。
    let out = run_odt_render("ext/lo_core_sw_qa_odfexport_tdf152710.odt");
    assert!(
        out.status.success(),
        "對照組 tdf152710 必須退出 0，實際 stderr: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("paragraph_count"),
        "對照組 stdout 應為投影 JSON，實際: {stdout:?}"
    );
}
