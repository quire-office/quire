//! zip 結構級對照器——roundtrip --check 的判定核心。
//!
//! 判定模型（任務規格：「zip 位元組級對照＋允許差異白名單」）：
//! - 對照單位是「結構欄位」而非整檔 hash：每個 entry 的
//!   name/method/crc/sizes/時間/属性/extra field/comment/gap，加上 archive comment。
//! - `ByteIdentical`：未觸碰存檔的理想結果——整檔完全相同。
//! - `UntouchedZeroLoss`：**通過門檻**。所有「未觸碰 entry」（白名單未放行改寫的）
//!   的壓縮位元組、extra、comment、gap 逐位元組相同，只有位移／尾端記錄／
//!   白名單內 entry 的重新壓縮允許不同。
//! - `Violation(...)`：未觸碰 entry 出現任何內容級差異 → 零失真要求失敗。
//!
//! 「未觸碰」由呼叫端（engine 的 roundtrip 指令）給定：一組允許變動的 part key。

use crate::zip_struct::{parse_extra_fields, ZipBundle, ZipEntry};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ZipStructureError {
    #[error("對照前解析失敗: {0}")]
    Open(String),
}

/// 單一 entry 的結構對照結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryDiff {
    pub name: String,
    /// 整條 central record 逐位元組相同（最強：連位移都沒變）。
    pub central_identical: bool,
    /// 壓縮位元組逐位元組相同。
    pub data_identical: bool,
    /// 解壓後明文相同（content 級等價）。
    pub content_identical: bool,
    /// local header 逐位元組相同。
    pub local_identical: bool,
    /// entry 間 gap（data descriptor／填充）逐位元組相同。
    pub gap_identical: bool,
    /// extra field 集合相同（id→bytes，不分 local/central）。
    pub extra_identical: bool,
    /// entry comment 相同。
    pub comment_identical: bool,
    /// method 相同。
    pub method_identical: bool,
    /// crc 相同。
    pub crc_identical: bool,
    /// 位移變了（central record 非 byte-identical 的主因）。
    pub offset_changed: bool,
}

impl EntryDiff {
    /// 「未觸碰 entry」的零失真判準：壓縮位元組＋extra＋comment＋gap＋method＋crc
    /// 全部相同（位移變是允許的，因為前面 entry 長度變會導致位移平移）。
    pub fn zero_loss(&self) -> bool {
        self.data_identical
            && self.extra_identical
            && self.comment_identical
            && self.gap_identical
            && self.method_identical
            && self.crc_identical
    }

    /// 對照摘要（人類可讀的一行）。
    pub fn summary(&self) -> String {
        let mut bad: Vec<&str> = Vec::new();
        if !self.data_identical {
            bad.push("data");
        }
        if !self.extra_identical {
            bad.push("extra");
        }
        if !self.comment_identical {
            bad.push("comment");
        }
        if !self.gap_identical {
            bad.push("gap");
        }
        if !self.method_identical {
            bad.push("method");
        }
        if !self.crc_identical {
            bad.push("crc");
        }
        if bad.is_empty() {
            if self.central_identical {
                "central byte-identical".to_string()
            } else {
                "content preserved (offset moved)".to_string()
            }
        } else {
            format!("DIFFERS: {}", bad.join(","))
        }
    }
}

/// 整檔對照結果。
#[derive(Debug, Clone)]
pub struct ZipDiff {
    pub byte_identical: bool,
    /// 整檔完全相同，或所有未觸碰 entry 通過零失真判準。
    pub pass: bool,
    /// 白名單內、內容真的變了的 entry 名。
    pub whitelisted_changed: Vec<String>,
    /// 未觸碰 entry 的零失真違規（entry 名＋差異摘要）。
    pub violations: Vec<String>,
    /// 只在一邊出現的 entry（缺 part 是違規；新增 part 是白名單外的改動）。
    pub missing: Vec<String>,
    pub added: Vec<String>,
    /// archive comment 不同。
    pub archive_comment_changed: bool,
    pub per_entry: BTreeMap<String, EntryDiff>,
}

impl ZipDiff {
    /// 人類可讀報告（roundtrip --check 的輸出）。
    pub fn report(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!(
            "byte_identical={} pass={}\n",
            self.byte_identical, self.pass
        ));
        if !self.whitelisted_changed.is_empty() {
            s.push_str(&format!(
                "whitelisted changed: {:?}\n",
                self.whitelisted_changed
            ));
        }
        if !self.missing.is_empty() {
            s.push_str(&format!("MISSING parts: {:?}\n", self.missing));
        }
        if !self.added.is_empty() {
            s.push_str(&format!("ADDED parts: {:?}\n", self.added));
        }
        if self.archive_comment_changed {
            s.push_str("archive comment differs\n");
        }
        for v in &self.violations {
            s.push_str(&format!("VIOLATION {v}\n"));
        }
        s
    }
}

/// extra field 區（local＋central 合併）轉成可比較的 map（id → data 串接順序保留）。
fn extra_map(e: &ZipEntry) -> Vec<(u16, Vec<u8>)> {
    let mut v = e.central_extra_fields();
    // local extra 只比 local_raw 裡的部分
    if e.local_raw.len() >= 30 {
        let nlen = u16::from_le_bytes([e.local_raw[26], e.local_raw[27]]) as usize;
        let elen = u16::from_le_bytes([e.local_raw[28], e.local_raw[29]]) as usize;
        let start = 30 + nlen;
        let end = (start + elen).min(e.local_raw.len());
        if start <= end {
            v.extend(parse_extra_fields(&e.local_raw[start..end]));
        }
    }
    v
}

/// 核心對照。`touched`：允許內容變動的 part key（白名單）；
/// 其餘 entry 一律套用零失真判準。
pub fn diff_zip_structure(
    original: &[u8],
    saved: &[u8],
    touched: &BTreeMap<String, ()>,
) -> Result<ZipDiff, ZipStructureError> {
    let a = ZipBundle::open(original).map_err(|e| ZipStructureError::Open(e.to_string()))?;
    let b = ZipBundle::open(saved).map_err(|e| ZipStructureError::Open(e.to_string()))?;

    let mut diff = ZipDiff {
        byte_identical: original == saved,
        pass: false,
        whitelisted_changed: Vec::new(),
        violations: Vec::new(),
        missing: Vec::new(),
        added: Vec::new(),
        archive_comment_changed: a.archive_comment_raw != b.archive_comment_raw,
        per_entry: BTreeMap::new(),
    };
    if diff.byte_identical {
        diff.pass = true;
        return Ok(diff);
    }

    let a_keys: Vec<String> = a.file_entries().map(|e| e.key()).collect();
    let b_keys: Vec<String> = b.file_entries().map(|e| e.key()).collect();
    for k in &a_keys {
        if !b_keys.contains(k) {
            diff.missing.push(k.clone());
        }
    }
    for k in &b_keys {
        if !a_keys.contains(k) {
            diff.added.push(k.clone());
        }
    }

    for k in &a_keys {
        let Some(ea) = a.entry(k) else { continue };
        let Some(eb) = b.entry(k) else { continue };
        let ed = EntryDiff {
            name: k.clone(),
            central_identical: ea.central_raw == eb.central_raw,
            data_identical: ea.data_raw == eb.data_raw,
            content_identical: ea.decoded().ok() == eb.decoded().ok(),
            local_identical: ea.local_raw == eb.local_raw,
            gap_identical: ea.gap_raw == eb.gap_raw,
            extra_identical: extra_map(ea) == extra_map(eb),
            comment_identical: ea.central_comment() == eb.central_comment(),
            method_identical: ea.method == eb.method,
            crc_identical: ea.crc == eb.crc,
            offset_changed: ea.local_header_offset != eb.local_header_offset,
        };
        let whitelisted = touched.contains_key(k);
        if whitelisted {
            if !ed.data_identical {
                diff.whitelisted_changed.push(k.clone());
            }
        } else if !ed.zero_loss() {
            diff.violations.push(format!("{k}: {}", ed.summary()));
        }
        diff.per_entry.insert(k.clone(), ed);
    }

    // part 集合必須相同（未白名單的缺 part 是硬違規；added 視同白名單外改動）。
    let mut ok = diff.violations.is_empty() && !diff.archive_comment_changed;
    // 白名單內允許移除；白名單外缺 part → 違規。
    for m in &diff.missing {
        if !touched.contains_key(m) {
            ok = false;
        }
    }
    // 白名單外新增 part → 違規（未觸碰部分多出不該存在的東西）。
    for ad in &diff.added {
        if !touched.contains_key(ad) {
            ok = false;
        }
    }
    diff.pass = ok;
    Ok(diff)
}

/// 便捷封裝：完全未觸碰（白名單為空）的對照——未觸碰存檔的門檻。
pub fn diff_untouched(original: &[u8], saved: &[u8]) -> Result<ZipDiff, ZipStructureError> {
    diff_zip_structure(original, saved, &BTreeMap::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zip_struct::METHOD_DEFLATE;

    fn sample() -> &'static [u8] {
        include_bytes!("../../../fixtures/sample.docx")
    }

    fn reserialize(
        bytes: &[u8],
        changes: &BTreeMap<String, crate::zip_struct::Change>,
        extras: &BTreeMap<String, Vec<u8>>,
    ) -> Vec<u8> {
        ZipBundle::open(bytes)
            .unwrap()
            .serialize(changes, extras)
            .unwrap()
    }

    #[test]
    fn untouched_sample_diff_is_byte_identical() {
        let saved = reserialize(sample(), &BTreeMap::new(), &BTreeMap::new());
        let d = diff_untouched(sample(), &saved).unwrap();
        assert!(d.byte_identical && d.pass);
    }

    #[test]
    fn editing_one_part_without_whitelist_is_a_violation() {
        let mut ch = BTreeMap::new();
        ch.insert(
            "word/document.xml".to_string(),
            crate::zip_struct::Change::Replace(b"<w:document/>".to_vec()),
        );
        let saved = reserialize(sample(), &ch, &BTreeMap::new());
        let d = diff_untouched(sample(), &saved).unwrap();
        assert!(!d.pass);
        assert!(d
            .violations
            .iter()
            .any(|v| v.starts_with("word/document.xml")));
    }

    #[test]
    fn same_edit_with_whitelist_passes() {
        let mut ch = BTreeMap::new();
        ch.insert(
            "word/document.xml".to_string(),
            crate::zip_struct::Change::Replace(b"<w:document/>".to_vec()),
        );
        let saved = reserialize(sample(), &ch, &BTreeMap::new());
        let mut touched = BTreeMap::new();
        touched.insert("word/document.xml".to_string(), ());
        let d = diff_zip_structure(sample(), &saved, &touched).unwrap();
        assert!(d.pass, "白名單放行後必須綠燈:\n{}", d.report());
        assert_eq!(d.whitelisted_changed, vec!["word/document.xml".to_string()]);
        // 其他 part 的壓縮位元組必須逐位元組不動
        let e = d.per_entry["[Content_Types].xml"].clone();
        assert!(e.data_identical && e.extra_identical && e.gap_identical);
    }

    #[test]
    fn missing_part_outside_whitelist_fails() {
        let mut ch = BTreeMap::new();
        ch.insert("_rels/.rels".to_string(), crate::zip_struct::Change::Remove);
        let saved = reserialize(sample(), &ch, &BTreeMap::new());
        let d = diff_untouched(sample(), &saved).unwrap();
        assert!(!d.pass);
        assert_eq!(d.missing, vec!["_rels/.rels".to_string()]);
    }

    #[test]
    fn added_part_outside_whitelist_fails() {
        let mut ex = BTreeMap::new();
        ex.insert("word/new.xml".to_string(), b"x".to_vec());
        let saved = reserialize(sample(), &BTreeMap::new(), &ex);
        let d = diff_untouched(sample(), &saved).unwrap();
        assert!(!d.pass);
        assert_eq!(d.added, vec!["word/new.xml".to_string()]);
    }

    #[test]
    fn method_flip_deflate_to_store_still_zero_loss_content() {
        // 把 entry 強改成 Store 存回：content 相同、crc 相同、method 不同 →
        // 未白名單時是違規（method_identical=false），白名單後 pass。
        let b = ZipBundle::open(sample()).unwrap();
        let e = b.entry("word/document.xml").unwrap();
        assert_eq!(e.method, METHOD_DEFLATE);
        let plain = e.decoded().unwrap();
        let mut ch = BTreeMap::new();
        ch.insert(
            "word/document.xml".to_string(),
            crate::zip_struct::Change::Replace(plain.clone()),
        );
        let saved = reserialize(sample(), &ch, &BTreeMap::new());
        let after = ZipBundle::open(&saved).unwrap();
        let eb = after.entry("word/document.xml").unwrap();
        assert_eq!(eb.decoded().unwrap(), plain, "覆寫後明文必須一致");
    }
}
