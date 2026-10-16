//! 位元組級 zip 結構層——round-trip 零失真的實作基礎。
//!
//! 為什麼不用 `zip` crate 的 writer 存回：它的 `FileOptions` 不保留 entry 級的
//! extra field／entry comment／data descriptor gap 等欄位，也沒有「未觸碰 entry
//! 原樣搬運原始位元組」的概念。無損 round-trip 的定義是「未觸碰的部分一位元組
//! 都不變」，所以這一層自己解析 central directory，把每個 entry 的
//! local header／壓縮資料／entry 間 gap／central record 全部留成原始位元組；
//! 存檔時未觸碰 entry 直接串接原始位元組，只有內容被改寫的 entry 才重建標頭。
//!
//! 純 std + flate2 + crc32fast，無檔案系統相依 → wasm32-unknown-unknown 可編譯。

use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use std::collections::BTreeMap;
use std::io::Read;
use std::io::Write;
use thiserror::Error;

/// 無壓縮。
pub const METHOD_STORE: u16 = 0;
/// RFC1951 raw deflate（DOCX 實務上只有 0 與 8 兩種）。
pub const METHOD_DEFLATE: u16 = 8;

const SIG_LOCAL: u32 = 0x0403_4b50;
const SIG_CENTRAL: u32 = 0x0201_4b50;
const SIG_EOCD: u32 = 0x0605_4b50;
const SIG_EOCD64: u32 = 0x0606_4b50;
const SIG_EOCD64_LOC: u32 = 0x0706_4b50;
const XID_ZIP64: u16 = 0x0001;
/// 位元組 3：data descriptor（sizes/crc 重複於資料之後）。
const FLAG_DATA_DESCRIPTOR: u16 = 0x0008;
/// 位元組 0：加密。本層拒絕（加密 entry round-trip 不在 Phase 0 範圍）。
const FLAG_ENCRYPTED: u16 = 0x0001;

const LOCAL_HEADER_LEN: usize = 30;
const CENTRAL_HEADER_LEN: usize = 46;
const EOCD_LEN: usize = 22;
const EOCD64_LEN: usize = 56;
const EOCD64_LOCATOR_LEN: usize = 20;
const ZIP64_MARKER: u32 = u32::MAX;

#[derive(Error, Debug)]
pub enum ZipError {
    #[error("zip 結構毀損: {0}")]
    Corrupt(&'static str),
    #[error("{name} 使用不支援的壓縮法 {method}（本層只支援 Stored/Deflate）")]
    UnsupportedMethod { name: String, method: u16 },
    #[error("{name} 是加密 entry，本層拒絕 round-trip")]
    Encrypted { name: String },
    #[error("deflate/crc 失敗: {0}")]
    Codec(String),
}

/// 一個 zip entry 的完整結構＋原始位元組。
/// `local_raw`/`data_raw`/`gap_raw`/`central_raw` 是零失真搬運的關鍵：
/// 未觸碰的 entry，存檔輸出＝這幾段原始位元組的直接串接。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ZipEntry {
    /// entry 名（原始位元組，可能不是合法 UTF-8）。
    pub name_raw: Vec<u8>,
    pub flags: u16,
    pub method: u16,
    pub crc: u32,
    pub uncompressed_size: u64,
    pub compressed_size: u64,
    pub dos_time: u16,
    pub dos_date: u16,
    pub version_made_by: u16,
    pub version_needed: u16,
    pub internal_attrs: u16,
    pub external_attrs: u32,
    /// central directory 記錄的 local header offset（zip64 已解析）。
    pub local_header_offset: u64,
    /// local header（固定區＋name＋extra）原始位元組。
    pub local_raw: Vec<u8>,
    /// 壓縮後的原始資料（未解壓）。
    pub data_raw: Vec<u8>,
    /// 本 entry 資料尾到下一個 entry 開頭（或 central directory）之間的位元組：
    /// data descriptor、對齊填充、垃圾都在這裡。原樣搬運才談得上零失真。
    pub gap_raw: Vec<u8>,
    /// central directory record 原始位元組（含 extra 與 entry comment）。
    pub central_raw: Vec<u8>,
    /// 凍結：存檔時永不改寫（撞名／目錄 entry 等「寧可不動，不可寫錯」的情況）。
    pub frozen: bool,
}

impl ZipEntry {
    /// 名稱以 `/` 結尾者為目錄 entry。
    pub fn is_dir(&self) -> bool {
        self.name_raw.ends_with(b"/")
    }

    /// 名稱的有損字串形式（parts map、changes 索引、對照器都用它）。
    pub fn key(&self) -> String {
        String::from_utf8_lossy(&self.name_raw).into_owned()
    }

    /// central record 的 extra field 之列 (header id, data)；本層只解析不改寫。
    pub fn central_extra_fields(&self) -> Vec<(u16, Vec<u8>)> {
        match central_parts(&self.central_raw) {
            Some((_, extra)) => parse_extra_fields(extra),
            None => Vec::new(),
        }
    }

    /// central record 尾端的 entry comment 原始位元組。
    pub fn central_comment(&self) -> &[u8] {
        central_comment_of(&self.central_raw)
    }

    /// 解壓後的明文（Stored 直接複製，Deflate 走 raw inflate）。
    pub fn decoded(&self) -> Result<Vec<u8>, ZipError> {
        decode_raw(&self.name_raw, self.method, &self.data_raw)
    }
}

/// 存檔時某個 key 的處置。
#[derive(Debug, Clone)]
pub enum Change {
    /// 用新明文覆寫（重新壓縮、重算 crc/sizes；其餘欄位沿用原 entry）。
    Replace(Vec<u8>),
    /// 從存檔中移除（凍結 entry 會忽略此指令）。
    Remove,
}

/// 整個 archive 的結構表示：entry 清單＋archive comment＋尾端記錄原始位元組。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ZipBundle {
    /// central directory 的 entry 順序（zip 的邏輯順序與輸出順序以此為準）。
    pub entries: Vec<ZipEntry>,
    /// 第一個 local header 之前的位元組（SFX stub 等；DOCX 通常為空）。
    pub prefix_raw: Vec<u8>,
    /// EOCD（含 archive comment）原始位元組。
    pub eocd_raw: Vec<u8>,
    /// ZIP64 EOCD record 原始位元組（若存在）。
    pub eocd64_raw: Vec<u8>,
    /// ZIP64 EOCD locator 原始位元組（若存在）。
    pub eocd64_locator_raw: Vec<u8>,
    /// archive comment（EOCD 尾部，原始位元組）。
    pub archive_comment_raw: Vec<u8>,
}

/// 單一 entry 的輸出計畫。
struct Planned {
    /// 來源 entry（新增 entry 為 None）。
    src: Option<ZipEntry>,
    /// true ＝ 內容未變：local/gap 輸出原始位元組，central 只 in-place 修位移。
    untouched: bool,
    name_raw: Vec<u8>,
    method: u16,
    crc: u32,
    usize_field: u64,
    /// 要輸出的壓縮資料（untouched 時＝data_raw）。
    data: Vec<u8>,
    /// 要輸出的 entry 間 gap（replaced/fresh 為空——原 gap 常是 data descriptor）。
    gap: Vec<u8>,
    dos_time: u16,
    dos_date: u16,
    version_made_by: u16,
    version_needed: u16,
    flags: u16,
    internal_attrs: u16,
    external_attrs: u32,
    /// 保留的 extra（已剔除 zip64 欄位）與 entry comment。
    extra_kept: Vec<u8>,
    comment_raw: Vec<u8>,
}

impl ZipBundle {
    /// 由 zip 位元組建立結構表示（只解析 central directory 與 local 標頭，
    /// entry 內容一律以壓縮位元組原樣保留）。
    pub fn open(bytes: &[u8]) -> Result<Self, ZipError> {
        let eocd_at =
            find_eocd(bytes).ok_or(ZipError::Corrupt("找不到 End of Central Directory"))?;
        let cmt_len = rd_u16(bytes, eocd_at + 20).ok_or(ZipError::Corrupt("EOCD 截斷"))? as usize;
        if eocd_at + EOCD_LEN + cmt_len > bytes.len() {
            return Err(ZipError::Corrupt("archive comment 長度越界"));
        }
        let archive_comment_raw = bytes[eocd_at + EOCD_LEN..eocd_at + EOCD_LEN + cmt_len].to_vec();
        let eocd_raw = bytes[eocd_at..eocd_at + EOCD_LEN + cmt_len].to_vec();

        let total16 = rd_u16(bytes, eocd_at + 8).ok_or(ZipError::Corrupt("EOCD 截斷"))?;
        let cd_size16 = rd_u32(bytes, eocd_at + 12).ok_or(ZipError::Corrupt("EOCD 截斷"))?;
        let cd_off16 = rd_u32(bytes, eocd_at + 16).ok_or(ZipError::Corrupt("EOCD 截斷"))?;
        let (mut total, mut cd_size, mut cd_off) =
            (total16 as u64, cd_size16 as u64, cd_off16 as u64);

        // ZIP64：任一欄位是 marker 就去讀 zip64 EOCD。
        let mut eocd64_raw = Vec::new();
        let mut eocd64_locator_raw = Vec::new();
        if total16 == u16::MAX || cd_size16 == ZIP64_MARKER || cd_off16 == ZIP64_MARKER {
            let loc_at = eocd_at
                .checked_sub(EOCD64_LOCATOR_LEN)
                .ok_or(ZipError::Corrupt("zip64 locator 位置為負"))?;
            if rd_u32(bytes, loc_at) != Some(SIG_EOCD64_LOC) {
                return Err(ZipError::Corrupt("zip64 locator 簽名不符"));
            }
            let z64_at =
                rd_u64(bytes, loc_at + 8).ok_or(ZipError::Corrupt("locator 截斷"))? as usize;
            if rd_u32(bytes, z64_at) != Some(SIG_EOCD64) {
                return Err(ZipError::Corrupt("zip64 EOCD 簽名不符"));
            }
            if z64_at + EOCD64_LEN > bytes.len() {
                return Err(ZipError::Corrupt("zip64 EOCD 越界"));
            }
            total = rd_u64(bytes, z64_at + 32).ok_or(ZipError::Corrupt("zip64 EOCD 截斷"))?;
            cd_size = rd_u64(bytes, z64_at + 40).ok_or(ZipError::Corrupt("zip64 EOCD 截斷"))?;
            cd_off = rd_u64(bytes, z64_at + 48).ok_or(ZipError::Corrupt("zip64 EOCD 截斷"))?;
            eocd64_raw = bytes[z64_at..z64_at + EOCD64_LEN].to_vec();
            eocd64_locator_raw = bytes[loc_at..loc_at + EOCD64_LOCATOR_LEN].to_vec();
        }

        let cd_start = cd_off as usize;
        let cd_end = cd_start
            .checked_add(cd_size as usize)
            .ok_or(ZipError::Corrupt("central directory 範圍溢位"))?;
        if cd_end > eocd_at {
            return Err(ZipError::Corrupt("central directory 與 EOCD 重疊"));
        }
        let central_block = &bytes[cd_start..cd_end];

        let mut entries = Vec::new();
        let mut pos = 0usize;
        for _ in 0..total {
            entries.push(parse_central_entry(central_block, &mut pos)?);
        }
        if pos != central_block.len() {
            return Err(ZipError::Corrupt("central directory 有多餘尾端位元組"));
        }

        // 依 central 記錄回填每個 entry 的 local header／壓縮資料／gap。
        let mut first_local = cd_start;
        for e in &mut entries {
            let lho = e.local_header_offset as usize;
            if lho + LOCAL_HEADER_LEN > cd_start {
                return Err(ZipError::Corrupt("local header offset 越界"));
            }
            if rd_u32(bytes, lho) != Some(SIG_LOCAL) {
                return Err(ZipError::Corrupt("local header 簽名不符"));
            }
            let nlen = rd_u16(bytes, lho + 26).ok_or(ZipError::Corrupt("local 截斷"))? as usize;
            let elen = rd_u16(bytes, lho + 28).ok_or(ZipError::Corrupt("local 截斷"))? as usize;
            let head_end = lho
                .checked_add(LOCAL_HEADER_LEN)
                .and_then(|v| v.checked_add(nlen))
                .and_then(|v| v.checked_add(elen))
                .ok_or(ZipError::Corrupt("local header 長度溢位"))?;
            if head_end > cd_start {
                return Err(ZipError::Corrupt("local name/extra 越界"));
            }
            let data_end = head_end
                .checked_add(e.compressed_size as usize)
                .ok_or(ZipError::Corrupt("壓縮資料長度溢位"))?;
            if data_end > cd_start {
                return Err(ZipError::Corrupt("壓縮資料越界"));
            }
            e.local_raw = bytes[lho..head_end].to_vec();
            e.data_raw = bytes[head_end..data_end].to_vec();
            first_local = first_local.min(lho);
        }
        // entry 間的 gap：本 entry 資料尾 → 下一個（依實體位移排序的）entry 開頭。
        let mut by_offset: Vec<usize> = (0..entries.len()).collect();
        by_offset.sort_by_key(|&i| entries[i].local_header_offset);
        for (rank, &idx) in by_offset.iter().enumerate() {
            let data_end = entries[idx].local_header_offset as usize
                + entries[idx].local_raw.len()
                + entries[idx].data_raw.len();
            let next_start = by_offset
                .get(rank + 1)
                .map(|&n| entries[n].local_header_offset as usize)
                .unwrap_or(cd_start);
            if next_start < data_end {
                return Err(ZipError::Corrupt("entry 資料互相重疊"));
            }
            entries[idx].gap_raw = bytes[data_end..next_start].to_vec();
        }
        let prefix_raw = if first_local < cd_start {
            bytes[..first_local].to_vec()
        } else {
            Vec::new()
        };

        let mut bundle = ZipBundle {
            entries,
            prefix_raw,
            eocd_raw,
            eocd64_raw,
            eocd64_locator_raw,
            archive_comment_raw,
        };
        bundle.mark_frozen();
        Ok(bundle)
    }

    /// 撞名（非法 UTF-8 名稱的有損 key 相同）與目錄 entry 一律凍結：
    /// parts map 以有損 key 索引，改寫這類 entry 有寫錯內容的風險。
    fn mark_frozen(&mut self) {
        let mut hits: BTreeMap<String, usize> = BTreeMap::new();
        for e in &self.entries {
            *hits.entry(e.key()).or_default() += 1;
        }
        for e in &mut self.entries {
            e.frozen = e.is_dir() || hits[&e.key()] > 1;
        }
    }

    /// 一般檔案 entry（排除目錄 entry）。
    pub fn file_entries(&self) -> impl Iterator<Item = &ZipEntry> {
        self.entries.iter().filter(|e| !e.is_dir())
    }

    pub fn entry(&self, name: &str) -> Option<&ZipEntry> {
        self.entries.iter().find(|e| e.key() == name)
    }

    pub fn part_names(&self) -> Vec<String> {
        self.file_entries().map(|e| e.key()).collect()
    }

    pub fn archive_comment(&self) -> &[u8] {
        &self.archive_comment_raw
    }

    /// 依 central directory 順序輸出整個 archive。
    /// `changes` 以 `key()` 索引；未列出的 entry 原樣搬運（壓縮位元組也不動）。
    /// `extras` 為新增 entry（deflate 壓縮、DOS 預設屬性、追加在尾端）。
    pub fn serialize(
        &self,
        changes: &BTreeMap<String, Change>,
        extras: &BTreeMap<String, Vec<u8>>,
    ) -> Result<Vec<u8>, ZipError> {
        let mut plan: Vec<Planned> = Vec::with_capacity(self.entries.len() + extras.len());
        let mut handled: BTreeMap<String, ()> = BTreeMap::new();

        for e in &self.entries {
            let key = e.key();
            let change = if e.frozen {
                None
            } else {
                changes.get(&key).cloned()
            };
            if change.is_some() {
                handled.insert(key, ());
            }
            match change {
                Some(Change::Remove) => continue,
                Some(Change::Replace(plain)) => {
                    let (data, crc, method) = encode_by_method(&plain, e.method)?;
                    // 覆寫成相同位元組 → 視為未觸碰（保住 byte-identical 保證）。
                    let same = crc == e.crc && data == e.data_raw;
                    if same {
                        plan.push(Planned::untouched(e));
                    } else {
                        plan.push(Planned::replaced(e, plain.len() as u64, data, crc, method));
                    }
                }
                None => plan.push(Planned::untouched(e)),
            }
        }
        // 新增 entry：既有 entry／已處理 key 不重複追加（BTreeMap 順序 → 可重現）。
        for (name, plain) in extras {
            if handled.contains_key(name) || self.entry(name).is_some() {
                continue;
            }
            let (data, crc, method) = encode_by_method(plain, METHOD_DEFLATE)?;
            plan.push(Planned::fresh(
                name.clone().into_bytes(),
                plain.len() as u64,
                data,
                crc,
                method,
            ));
        }

        // 1) local headers＋資料＋gap，記錄每個 entry 的新位移。
        let mut out: Vec<u8> = Vec::with_capacity(
            self.prefix_raw.len()
                + plan
                    .iter()
                    .map(|p| p.local_len_estimate() + p.data.len() + p.gap.len())
                    .sum::<usize>()
                + 4096,
        );
        out.extend_from_slice(&self.prefix_raw);
        let mut offsets: Vec<u64> = Vec::with_capacity(plan.len());
        for p in &plan {
            offsets.push(out.len() as u64);
            let local = match &p.src {
                Some(e) if p.untouched => e.local_raw.clone(),
                _ => build_local_with_extra(
                    &p.name_raw,
                    p.method,
                    p.crc,
                    p.usize_field,
                    p.data.len() as u64,
                    p.dos_time,
                    p.dos_date,
                    p.version_needed,
                    p.flags & !FLAG_DATA_DESCRIPTOR,
                    &p.extra_kept,
                ),
            };
            out.extend_from_slice(&local);
            out.extend_from_slice(&p.data);
            out.extend_from_slice(&p.gap);
        }

        // 2) central records：未觸碰者逐位元組沿用、只 in-place 修位移；覆寫者重建。
        let cd_start = out.len() as u64;
        for (p, &off) in plan.iter().zip(&offsets) {
            let rec = match &p.src {
                Some(e) if p.untouched => patch_central_offset(&e.central_raw, off)?,
                _ => build_central(
                    &p.name_raw,
                    p.method,
                    p.crc,
                    p.usize_field,
                    p.data.len() as u64,
                    off,
                    p.dos_time,
                    p.dos_date,
                    p.version_made_by,
                    p.version_needed,
                    p.flags & !FLAG_DATA_DESCRIPTOR,
                    p.internal_attrs,
                    p.external_attrs,
                    &p.extra_kept,
                    &p.comment_raw,
                ),
            };
            out.extend_from_slice(&rec);
        }
        let cd_size = out.len() as u64 - cd_start;
        let count = plan.len() as u64;

        // 3) 尾端記錄（zip64 與否沿用原 archive 的決定）。
        out.extend(self.tail_records(cd_size, cd_start, count)?);
        Ok(out)
    }

    /// 未觸碰存檔時輸出＝原始尾端位元組（覆寫同值→逐字節相同）；
    /// 有增刪/改寫時只重寫 count/cd_size/cd_off 這幾個欄位。
    fn tail_records(&self, cd_size: u64, cd_off: u64, count: u64) -> Result<Vec<u8>, ZipError> {
        let mut out = Vec::new();
        if self.eocd64_raw.is_empty() {
            let mut eocd = self.eocd_raw.clone();
            if eocd.len() < EOCD_LEN {
                return Err(ZipError::Corrupt("EOCD 過短"));
            }
            if count > u16::MAX as u64 || cd_size > u32::MAX as u64 || cd_off > u32::MAX as u64 {
                return Err(ZipError::Corrupt(
                    "存檔結果超出 32 位元組欄位但原 archive 無 zip64（Phase 0 corpus 不應發生）",
                ));
            }
            eocd[8..10].copy_from_slice(&(count as u16).to_le_bytes());
            eocd[10..12].copy_from_slice(&(count as u16).to_le_bytes());
            eocd[12..16].copy_from_slice(&(cd_size as u32).to_le_bytes());
            eocd[16..20].copy_from_slice(&(cd_off as u32).to_le_bytes());
            out.extend_from_slice(&eocd);
            return Ok(out);
        }
        let mut z64 = self.eocd64_raw.clone();
        let mut loc = self.eocd64_locator_raw.clone();
        if z64.len() < EOCD64_LEN || loc.len() < EOCD64_LOCATOR_LEN {
            return Err(ZipError::Corrupt("zip64 記錄過短"));
        }
        // zip64 EOCD：records(24/32)、cd size(40)、cd offset(48)。
        z64[24..32].copy_from_slice(&count.to_le_bytes());
        z64[32..40].copy_from_slice(&count.to_le_bytes());
        z64[40..48].copy_from_slice(&cd_size.to_le_bytes());
        z64[48..56].copy_from_slice(&cd_off.to_le_bytes());
        // zip64 EOCD 的實體位置（緊接 central 之後）＝locator 要指的地方。
        let z64_at = cd_off + cd_size;
        // locator：zip64 EOCD 起始位移(8..16)。
        loc[8..16].copy_from_slice(&z64_at.to_le_bytes());
        let mut eocd = self.eocd_raw.clone();
        if eocd.len() < EOCD_LEN {
            return Err(ZipError::Corrupt("EOCD 過短"));
        }
        // zip64 archive 的標準 EOCD 欄位通常是 marker：保留原始「形式」
        // （原欄位是 marker 就繼續寫 marker，否則寫實際值）→ 未觸碰時 byte-identical。
        let cnt = if rd_u16(&eocd, 8) == Some(u16::MAX) || count > u16::MAX as u64 {
            u16::MAX
        } else {
            count as u16
        };
        let cds = if rd_u32(&eocd, 12) == Some(ZIP64_MARKER) || cd_size > u32::MAX as u64 {
            ZIP64_MARKER
        } else {
            cd_size as u32
        };
        let cdo = if rd_u32(&eocd, 16) == Some(ZIP64_MARKER) || cd_off > u32::MAX as u64 {
            ZIP64_MARKER
        } else {
            cd_off as u32
        };
        eocd[8..10].copy_from_slice(&cnt.to_le_bytes());
        eocd[10..12].copy_from_slice(&cnt.to_le_bytes());
        eocd[12..16].copy_from_slice(&cds.to_le_bytes());
        eocd[16..20].copy_from_slice(&cdo.to_le_bytes());
        out.extend_from_slice(&z64);
        out.extend_from_slice(&loc);
        out.extend_from_slice(&eocd);
        Ok(out)
    }
}

impl Planned {
    fn untouched(e: &ZipEntry) -> Planned {
        Planned {
            src: Some(e.clone()),
            untouched: true,
            name_raw: e.name_raw.clone(),
            method: e.method,
            crc: e.crc,
            usize_field: e.uncompressed_size,
            data: e.data_raw.clone(),
            gap: e.gap_raw.clone(),
            dos_time: e.dos_time,
            dos_date: e.dos_date,
            version_made_by: e.version_made_by,
            version_needed: e.version_needed,
            flags: e.flags,
            internal_attrs: e.internal_attrs,
            external_attrs: e.external_attrs,
            extra_kept: Vec::new(),
            comment_raw: Vec::new(),
        }
    }

    /// 覆寫內容：時間/属性/extra（去 zip64）/comment 沿用原 entry，sizes/crc/method 更新。
    fn replaced(e: &ZipEntry, plain_len: u64, data: Vec<u8>, crc: u32, method: u16) -> Planned {
        Planned {
            src: Some(e.clone()),
            untouched: false,
            name_raw: e.name_raw.clone(),
            method,
            crc,
            usize_field: plain_len,
            data,
            gap: Vec::new(),
            dos_time: e.dos_time,
            dos_date: e.dos_date,
            version_made_by: e.version_made_by,
            version_needed: e.version_needed,
            flags: e.flags,
            internal_attrs: e.internal_attrs,
            external_attrs: e.external_attrs,
            extra_kept: strip_zip64_extra(local_extra_of(e)),
            comment_raw: central_comment_of(&e.central_raw).to_vec(),
        }
    }

    fn fresh(name_raw: Vec<u8>, plain_len: u64, data: Vec<u8>, crc: u32, method: u16) -> Planned {
        Planned {
            src: None,
            untouched: false,
            name_raw,
            method,
            crc,
            usize_field: plain_len,
            data,
            gap: Vec::new(),
            dos_time: 0,
            dos_date: 0,
            version_made_by: 0x031E, // UNIX 2.0（與 DOCX 產出器一致的字面預設）
            version_needed: 20,
            flags: 0,
            internal_attrs: 0,
            external_attrs: 0o644 << 16,
            extra_kept: Vec::new(),
            comment_raw: Vec::new(),
        }
    }

    fn local_len_estimate(&self) -> usize {
        match &self.src {
            Some(e) if self.untouched => e.local_raw.len(),
            _ => LOCAL_HEADER_LEN + self.name_raw.len() + self.extra_kept.len(),
        }
    }
}

// ---------------------------------------------------------------- 標頭組建

#[allow(clippy::too_many_arguments)]
fn build_local_with_extra(
    name: &[u8],
    method: u16,
    crc: u32,
    usize_field: u64,
    csize: u64,
    dos_time: u16,
    dos_date: u16,
    version_needed: u16,
    flags: u16,
    extra0: &[u8],
) -> Vec<u8> {
    let needs64 = needs_zip64(usize_field, csize, 0);
    let mut extra = extra0.to_vec();
    if needs64 {
        push_zip64_extra(&mut extra, usize_field, csize, None);
    }
    let mut out = Vec::with_capacity(LOCAL_HEADER_LEN + name.len() + extra.len());
    push_u32(&mut out, SIG_LOCAL);
    push_u16(&mut out, version_needed.max(if needs64 { 45 } else { 10 }));
    push_u16(&mut out, flags);
    push_u16(&mut out, method);
    push_u16(&mut out, dos_time);
    push_u16(&mut out, dos_date);
    push_u32(&mut out, crc);
    push_u32(&mut out, fit32(csize));
    push_u32(&mut out, fit32(usize_field));
    push_u16(&mut out, name.len() as u16);
    push_u16(&mut out, extra.len() as u16);
    out.extend_from_slice(name);
    out.extend_from_slice(&extra);
    out
}

#[allow(clippy::too_many_arguments)]
fn build_central(
    name: &[u8],
    method: u16,
    crc: u32,
    usize_field: u64,
    csize: u64,
    offset: u64,
    dos_time: u16,
    dos_date: u16,
    version_made_by: u16,
    version_needed: u16,
    flags: u16,
    internal_attrs: u16,
    external_attrs: u32,
    extra0: &[u8],
    comment: &[u8],
) -> Vec<u8> {
    let needs64 = needs_zip64(usize_field, csize, offset);
    let mut extra = extra0.to_vec();
    if needs64 {
        push_zip64_extra(&mut extra, usize_field, csize, Some(offset));
    }
    let mut out = Vec::with_capacity(CENTRAL_HEADER_LEN + name.len() + extra.len() + comment.len());
    push_u32(&mut out, SIG_CENTRAL);
    push_u16(&mut out, version_made_by);
    push_u16(&mut out, version_needed.max(if needs64 { 45 } else { 10 }));
    push_u16(&mut out, flags);
    push_u16(&mut out, method);
    push_u16(&mut out, dos_time);
    push_u16(&mut out, dos_date);
    push_u32(&mut out, crc);
    push_u32(&mut out, fit32(csize));
    push_u32(&mut out, fit32(usize_field));
    push_u16(&mut out, name.len() as u16);
    push_u16(&mut out, extra.len() as u16);
    push_u16(&mut out, comment.len() as u16);
    push_u16(&mut out, 0); // start disk
    push_u16(&mut out, internal_attrs);
    push_u32(&mut out, external_attrs);
    push_u32(&mut out, fit32(offset));
    out.extend_from_slice(name);
    out.extend_from_slice(&extra);
    out.extend_from_slice(comment);
    out
}

/// 未變動 entry 的 central record：逐位元組沿用，只把 local header 位移寫回新值
/// （標準 4 位元組欄位，或 zip64 extra 內的 8 位元組 slot——皆 in-place，長度不變）。
fn patch_central_offset(central_raw: &[u8], offset: u64) -> Result<Vec<u8>, ZipError> {
    let mut out = central_raw.to_vec();
    // offset 欄位不是 marker → 直接修標準 4 位元組欄位（zip64 extra 即使存在也不含 offset）。
    if rd_u32(&out, 42) != Some(ZIP64_MARKER) {
        if offset > u32::MAX as u64 {
            return Err(ZipError::Corrupt(
                "entry 位移超過 4GB 但無 zip64 offset 欄位",
            ));
        }
        out[42..46].copy_from_slice(&(offset as u32).to_le_bytes());
        return Ok(out);
    }
    let (extra_at, extra) = central_parts(&out).ok_or(ZipError::Corrupt("central record 畸形"))?;
    let mut slot: Option<usize> = None;
    let mut cur = extra;
    let mut rel = 0usize;
    while cur.len() >= 4 {
        let id = rd_u16(cur, 0).ok_or(ZipError::Corrupt("extra 截斷"))?;
        let len = rd_u16(cur, 2).ok_or(ZipError::Corrupt("extra 截斷"))? as usize;
        if cur.len() < 4 + len {
            break;
        }
        if id == XID_ZIP64 {
            // zip64 body 順序 uncompressed, compressed, offset——某欄存在 iff
            // 對應標準欄位（24=usize、20=csize、42=offset）是 marker。
            let mut at = extra_at + rel + 4;
            if rd_u32(&out, 24) == Some(ZIP64_MARKER) {
                at += 8;
            }
            if rd_u32(&out, 20) == Some(ZIP64_MARKER) {
                at += 8;
            }
            slot = Some(at);
            break;
        }
        rel += 4 + len;
        cur = &cur[4 + len..];
    }
    match slot {
        Some(at) => {
            if at + 8 > out.len() {
                return Err(ZipError::Corrupt("zip64 extra 內 offset 欄位越界"));
            }
            out[at..at + 8].copy_from_slice(&offset.to_le_bytes());
        }
        None => return Err(ZipError::Corrupt("offset 欄位是 marker 但無 zip64 extra")),
    }
    Ok(out)
}

// ---------------------------------------------------------------- 解析 helpers

fn find_eocd(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < EOCD_LEN {
        return None;
    }
    let floor = bytes.len().saturating_sub(EOCD_LEN + u16::MAX as usize);
    let mut at = bytes.len() - EOCD_LEN;
    loop {
        if rd_u32(bytes, at) == Some(SIG_EOCD) {
            if let Some(cmt_len) = rd_u16(bytes, at + 20) {
                if at + EOCD_LEN + cmt_len as usize == bytes.len() {
                    return Some(at);
                }
            }
        }
        if at == floor {
            return None;
        }
        at -= 1;
    }
}

fn parse_central_entry(block: &[u8], pos: &mut usize) -> Result<ZipEntry, ZipError> {
    let at = *pos;
    if at + CENTRAL_HEADER_LEN > block.len() {
        return Err(ZipError::Corrupt("central record 截斷"));
    }
    if rd_u32(block, at) != Some(SIG_CENTRAL) {
        return Err(ZipError::Corrupt("central record 簽名不符"));
    }
    let version_made_by = rd_u16(block, at + 4).unwrap_or(0);
    let version_needed = rd_u16(block, at + 6).unwrap_or(0);
    let flags = rd_u16(block, at + 8).unwrap_or(0);
    let method = rd_u16(block, at + 10).unwrap_or(0);
    let dos_time = rd_u16(block, at + 12).unwrap_or(0);
    let dos_date = rd_u16(block, at + 14).unwrap_or(0);
    let crc = rd_u32(block, at + 16).unwrap_or(0);
    // 規格順序：20=compressed、24=uncompressed（很容易讀反，特別標註）。
    let mut csize_field = rd_u32(block, at + 20).unwrap_or(0) as u64;
    let mut usize_field = rd_u32(block, at + 24).unwrap_or(0) as u64;
    let nlen = rd_u16(block, at + 28).unwrap_or(0) as usize;
    let elen = rd_u16(block, at + 30).unwrap_or(0) as usize;
    let clen = rd_u16(block, at + 32).unwrap_or(0) as usize;
    let internal_attrs = rd_u16(block, at + 36).unwrap_or(0);
    let external_attrs = rd_u32(block, at + 38).unwrap_or(0);
    let mut offset_field = rd_u32(block, at + 42).unwrap_or(0) as u64;

    let name_at = at + CENTRAL_HEADER_LEN;
    let extra_at = name_at + nlen;
    let comment_at = extra_at + elen;
    let rec_end = comment_at + clen;
    if rec_end > block.len() {
        return Err(ZipError::Corrupt("central record 名稱/extra/comment 越界"));
    }
    let name_raw = block[name_at..extra_at].to_vec();
    let extra_raw = &block[extra_at..comment_at];
    let central_raw = block[at..rec_end].to_vec();
    *pos = rec_end;

    let name_for_err = String::from_utf8_lossy(&name_raw).into_owned();
    if method != METHOD_STORE && method != METHOD_DEFLATE {
        return Err(ZipError::UnsupportedMethod {
            name: name_for_err,
            method,
        });
    }
    if flags & FLAG_ENCRYPTED != 0 {
        return Err(ZipError::Encrypted { name: name_for_err });
    }

    // zip64 覆蓋：只取被寫成 marker 的欄位（固定順序 usize, csize, offset）。
    if usize_field == ZIP64_MARKER as u64
        || csize_field == ZIP64_MARKER as u64
        || offset_field == ZIP64_MARKER as u64
    {
        for (id, data) in parse_extra_fields(extra_raw) {
            if id != XID_ZIP64 {
                continue;
            }
            let mut i = 0usize;
            if usize_field == ZIP64_MARKER as u64 {
                usize_field = rd_u64(&data, i).ok_or(ZipError::Corrupt("zip64 extra 截斷"))?;
                i += 8;
            }
            if csize_field == ZIP64_MARKER as u64 {
                csize_field = rd_u64(&data, i).ok_or(ZipError::Corrupt("zip64 extra 截斷"))?;
                i += 8;
            }
            if offset_field == ZIP64_MARKER as u64 {
                offset_field = rd_u64(&data, i).ok_or(ZipError::Corrupt("zip64 extra 截斷"))?;
            }
        }
    }

    Ok(ZipEntry {
        name_raw,
        flags,
        method,
        crc,
        uncompressed_size: usize_field,
        compressed_size: csize_field,
        dos_time,
        dos_date,
        version_made_by,
        version_needed,
        internal_attrs,
        external_attrs,
        local_header_offset: offset_field,
        local_raw: Vec::new(),
        data_raw: Vec::new(),
        gap_raw: Vec::new(),
        central_raw,
        frozen: false,
    })
}

/// extra field 區的 TLV 走訪：(header id, data) 序列；畸形即停止。
pub fn parse_extra_fields(mut data: &[u8]) -> Vec<(u16, Vec<u8>)> {
    let mut out = Vec::new();
    while data.len() >= 4 {
        let id = rd_u16(data, 0).unwrap_or(0);
        let len = rd_u16(data, 2).unwrap_or(0) as usize;
        if data.len() < 4 + len {
            break;
        }
        out.push((id, data[4..4 + len].to_vec()));
        data = &data[4 + len..];
    }
    out
}

fn central_parts(raw: &[u8]) -> Option<(usize, &[u8])> {
    if raw.len() < CENTRAL_HEADER_LEN {
        return None;
    }
    let nlen = rd_u16(raw, 28)? as usize;
    let elen = rd_u16(raw, 30)? as usize;
    let start = CENTRAL_HEADER_LEN.checked_add(nlen)?;
    let end = start.checked_add(elen)?;
    if end > raw.len() {
        return None;
    }
    Some((start, &raw[start..end]))
}

fn local_extra_of(e: &ZipEntry) -> &[u8] {
    if e.local_raw.len() < LOCAL_HEADER_LEN {
        return &[];
    }
    let nlen = rd_u16(&e.local_raw, 26).unwrap_or(0) as usize;
    let elen = rd_u16(&e.local_raw, 28).unwrap_or(0) as usize;
    let start = LOCAL_HEADER_LEN + nlen;
    let end = start.saturating_add(elen).min(e.local_raw.len());
    if start > end {
        return &[];
    }
    &e.local_raw[start..end]
}

fn central_comment_of(raw: &[u8]) -> &[u8] {
    if raw.len() < CENTRAL_HEADER_LEN {
        return &[];
    }
    if central_parts(raw).is_none() {
        return &[];
    }
    let nlen = rd_u16(raw, 28).unwrap_or(0) as usize;
    let elen = rd_u16(raw, 30).unwrap_or(0) as usize;
    let clen = rd_u16(raw, 32).unwrap_or(0) as usize;
    let start = CENTRAL_HEADER_LEN + nlen + elen;
    let end = start.saturating_add(clen).min(raw.len());
    if start > end {
        return &[];
    }
    &raw[start..end]
}

fn decode_raw(name_raw: &[u8], method: u16, raw: &[u8]) -> Result<Vec<u8>, ZipError> {
    let name = String::from_utf8_lossy(name_raw).into_owned();
    match method {
        METHOD_STORE => Ok(raw.to_vec()),
        METHOD_DEFLATE => {
            let mut dec = DeflateDecoder::new(raw);
            let mut buf = Vec::new();
            dec.read_to_end(&mut buf)
                .map_err(|e| ZipError::Codec(format!("{name}: {e}")))?;
            Ok(buf)
        }
        m => Err(ZipError::UnsupportedMethod { name, method: m }),
    }
}

/// 壓縮新明文，回傳 (compressed, crc, effective method)。deflate 沒比較小就退回 Stored。
fn encode_by_method(plain: &[u8], want: u16) -> Result<(Vec<u8>, u32, u16), ZipError> {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(plain);
    let crc = hasher.finalize();
    match want {
        METHOD_STORE => Ok((plain.to_vec(), crc, METHOD_STORE)),
        METHOD_DEFLATE => {
            let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
            enc.write_all(plain)
                .map_err(|e| ZipError::Codec(e.to_string()))?;
            let data = enc.finish().map_err(|e| ZipError::Codec(e.to_string()))?;
            if data.len() >= plain.len() {
                Ok((plain.to_vec(), crc, METHOD_STORE))
            } else {
                Ok((data, crc, METHOD_DEFLATE))
            }
        }
        m => Err(ZipError::UnsupportedMethod {
            name: "(新內容)".into(),
            method: m,
        }),
    }
}

fn needs_zip64(usize_field: u64, csize: u64, offset: u64) -> bool {
    usize_field > u32::MAX as u64 || csize > u32::MAX as u64 || offset > u32::MAX as u64
}

fn push_zip64_extra(extra: &mut Vec<u8>, usize_field: u64, csize: u64, offset: Option<u64>) {
    let mut body = Vec::with_capacity(24);
    push_u64(&mut body, usize_field);
    push_u64(&mut body, csize);
    if let Some(o) = offset {
        push_u64(&mut body, o);
    }
    push_u16(extra, XID_ZIP64);
    push_u16(extra, body.len() as u16);
    extra.extend_from_slice(&body);
}

fn strip_zip64_extra(extra: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for (id, data) in parse_extra_fields(extra) {
        if id == XID_ZIP64 {
            continue;
        }
        push_u16(&mut out, id);
        push_u16(&mut out, data.len() as u16);
        out.extend_from_slice(&data);
    }
    out
}

fn fit32(v: u64) -> u32 {
    if v > u32::MAX as u64 {
        ZIP64_MARKER
    } else {
        v as u32
    }
}

fn rd_u16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn rd_u32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn rd_u64(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

fn push_u16(b: &mut Vec<u8>, v: u16) {
    b.extend_from_slice(&v.to_le_bytes());
}

fn push_u32(b: &mut Vec<u8>, v: u32) {
    b.extend_from_slice(&v.to_le_bytes());
}

fn push_u64(b: &mut Vec<u8>, v: u64) {
    b.extend_from_slice(&v.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// 手工拼 zip：每個 entry 直接給壓縮位元組（不依賴任何 zip 函式庫）。
    struct Builder {
        buf: Vec<u8>,
        centrals: Vec<Vec<u8>>,
        count: usize,
    }

    impl Builder {
        fn new(prefix: &[u8]) -> Builder {
            let mut buf = prefix.to_vec();
            buf.reserve(4096);
            Builder {
                buf,
                centrals: Vec::new(),
                count: 0,
            }
        }

        #[allow(clippy::too_many_arguments)]
        fn entry(
            &mut self,
            name: &[u8],
            method: u16,
            plain: &[u8],
            extra_local: &[u8],
            extra_central: &[u8],
            comment: &[u8],
            gap: &[u8],
            flags: u16,
            external_attrs: u32,
        ) {
            let (data, crc) = match method {
                METHOD_STORE => (plain.to_vec(), crc_of(plain)),
                METHOD_DEFLATE => {
                    let d = deflate(plain);
                    (d, crc_of(plain))
                }
                _ => unreachable!(),
            };
            let offset = self.buf.len() as u64;
            // local
            push_u32(&mut self.buf, SIG_LOCAL);
            push_u16(&mut self.buf, 20);
            push_u16(&mut self.buf, flags);
            push_u16(&mut self.buf, method);
            push_u16(&mut self.buf, 0x1234);
            push_u16(&mut self.buf, 0x5678);
            push_u32(&mut self.buf, crc);
            push_u32(&mut self.buf, fit32(data.len() as u64));
            push_u32(&mut self.buf, fit32(plain.len() as u64));
            push_u16(&mut self.buf, name.len() as u16);
            push_u16(&mut self.buf, extra_local.len() as u16);
            self.buf.extend_from_slice(name);
            self.buf.extend_from_slice(extra_local);
            self.buf.extend_from_slice(&data);
            self.buf.extend_from_slice(gap);
            // central
            let mut c = Vec::new();
            push_u32(&mut c, SIG_CENTRAL);
            push_u16(&mut c, 0x031E);
            push_u16(&mut c, 20);
            push_u16(&mut c, flags);
            push_u16(&mut c, method);
            push_u16(&mut c, 0x1234);
            push_u16(&mut c, 0x5678);
            push_u32(&mut c, crc);
            push_u32(&mut c, fit32(data.len() as u64));
            push_u32(&mut c, fit32(plain.len() as u64));
            push_u16(&mut c, name.len() as u16);
            push_u16(&mut c, extra_central.len() as u16);
            push_u16(&mut c, comment.len() as u16);
            push_u16(&mut c, 0);
            push_u16(&mut c, 0);
            push_u32(&mut c, external_attrs);
            push_u32(&mut c, fit32(offset));
            c.extend_from_slice(name);
            c.extend_from_slice(extra_central);
            c.extend_from_slice(comment);
            self.centrals.push(c);
            self.count += 1;
        }

        fn finish(&mut self, archive_comment: &[u8]) -> Vec<u8> {
            let cd_off = self.buf.len() as u64;
            for c in &self.centrals {
                self.buf.extend_from_slice(c);
            }
            let cd_size = self.buf.len() as u64 - cd_off;
            push_u32(&mut self.buf, SIG_EOCD);
            push_u16(&mut self.buf, 0);
            push_u16(&mut self.buf, 0);
            push_u16(&mut self.buf, self.count as u16);
            push_u16(&mut self.buf, self.count as u16);
            push_u32(&mut self.buf, cd_size as u32);
            push_u32(&mut self.buf, cd_off as u32);
            push_u16(&mut self.buf, archive_comment.len() as u16);
            self.buf.extend_from_slice(archive_comment);
            self.buf.clone()
        }
    }

    fn crc_of(data: &[u8]) -> u32 {
        let mut h = crc32fast::Hasher::new();
        h.update(data);
        h.finalize()
    }

    fn deflate(plain: &[u8]) -> Vec<u8> {
        use std::io::Write as _;
        let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
        enc.write_all(plain).unwrap();
        enc.finish().unwrap()
    }

    fn empty_changes() -> (BTreeMap<String, Change>, BTreeMap<String, Vec<u8>>) {
        (BTreeMap::new(), BTreeMap::new())
    }

    /// 一個什麼怪都有的 archive：前綴 stub、Stored＋Deflate、目錄 entry、
    /// entry comment、自製 extra field、data descriptor gap、archive comment。
    fn kitchen_sink() -> Vec<u8> {
        let mut b = Builder::new(b"StubPrefix!");
        b.entry(
            b"[Content_Types].xml",
            METHOD_DEFLATE,
            b"<Types/>",
            b"",
            b"",
            b"",
            b"",
            0,
            0x01800000,
        );
        b.entry(
            b"word/",
            METHOD_STORE,
            b"",
            b"",
            b"",
            b"",
            b"",
            0,
            0x10000010,
        );
        {
            // 帶自製 extra field（0x7075）＋comment 的 entry
            let extra_local = {
                let mut v = Vec::new();
                push_u16(&mut v, 0x7075);
                push_u16(&mut v, 3);
                v.extend_from_slice(b"abc");
                v
            };
            let extra_central = extra_local.clone();
            b.entry(
                b"word/document.xml",
                METHOD_DEFLATE,
                b"<w:document/>",
                &extra_local,
                &extra_central,
                b"note",
                b"",
                0,
                0x01A40000,
            );
        }
        {
            // data descriptor 風格：flag bit3＋資料後的 descriptor gap
            let mut gap = Vec::new();
            push_u32(&mut gap, 0x0807_4b50);
            push_u32(&mut gap, 0);
            push_u32(&mut gap, 0);
            push_u32(&mut gap, 0);
            b.entry(
                b"word/settings.xml",
                METHOD_STORE,
                b"<settings/>",
                b"",
                b"",
                b"",
                &gap,
                0x0008,
                0x01800000,
            );
        }
        b.finish(b"hello zip")
    }

    #[test]
    fn untouched_round_trip_is_byte_identical() {
        let src = kitchen_sink();
        let bundle = ZipBundle::open(&src).expect("kitchen sink 解析失敗");
        assert_eq!(bundle.entries.len(), 4);
        assert_eq!(bundle.part_names().len(), 3, "目錄 entry 不算 part");
        assert_eq!(bundle.archive_comment(), b"hello zip");
        let (ch, ex) = empty_changes();
        let out = bundle.serialize(&ch, &ex).expect("serialize 失敗");
        assert_eq!(out, src, "未觸碰存檔必須位元組級相同");
    }

    #[test]
    fn replace_only_touches_the_edited_entry() {
        let src = kitchen_sink();
        let bundle = ZipBundle::open(&src).expect("open");
        let mut ch = BTreeMap::new();
        ch.insert(
            "word/settings.xml".to_string(),
            Change::Replace(b"<settings changed/>".to_vec()),
        );
        let (_, ex) = empty_changes();
        let out = bundle.serialize(&ch, &ex).expect("serialize");
        let again = ZipBundle::open(&out).expect("存檔後再解析失敗");
        assert_eq!(
            again.entry("word/settings.xml").unwrap().decoded().unwrap(),
            b"<settings changed/>"
        );
        // 其餘 entry 的 central record 與原始逐字節相同（位移也相同，因為只有這一個 entry 長度變、
        // 且它排在 central……位移可能變，所以比 content/crc/extra 而不是比位移）。
        for name in ["[Content_Types].xml", "word/document.xml"] {
            let a = bundle.entry(name).unwrap();
            let c = again.entry(name).unwrap();
            assert_eq!(a.central_raw, c.central_raw, "{name} central 應逐字節不變");
            assert_eq!(a.data_raw, c.data_raw, "{name} 壓縮位元組應不變");
        }
        // entry comment／extra 必須活過 round-trip
        let doc = again.entry("word/document.xml").unwrap();
        assert!(doc
            .central_extra_fields()
            .iter()
            .any(|(id, d)| *id == 0x7075 && d == b"abc"));
        assert_eq!(central_comment_of(&doc.central_raw), b"note");
    }

    #[test]
    fn remove_drops_the_entry() {
        let src = kitchen_sink();
        let bundle = ZipBundle::open(&src).expect("open");
        let mut ch = BTreeMap::new();
        ch.insert("word/settings.xml".to_string(), Change::Remove);
        let (_, ex) = empty_changes();
        let out = bundle.serialize(&ch, &ex).expect("serialize");
        let again = ZipBundle::open(&out).expect("open after remove");
        assert!(again.entry("word/settings.xml").is_none());
        assert!(again.entry("word/document.xml").is_some());
        // 目錄 entry 的 Remove 被凍結規則擋下（這裡沒下 Remove，理應還在）
        assert!(again.entry("word/").is_some());
    }

    #[test]
    fn extra_entry_appended_and_readable() {
        let src = kitchen_sink();
        let bundle = ZipBundle::open(&src).expect("open");
        let (ch, mut ex) = empty_changes();
        ex.insert("word/extra.txt".to_string(), b"brand new".to_vec());
        let out = bundle.serialize(&ch, &ex).expect("serialize");
        let again = ZipBundle::open(&out).expect("open after add");
        assert_eq!(
            again.entry("word/extra.txt").unwrap().decoded().unwrap(),
            b"brand new"
        );
        // 既有 entry 的內容完全沒被擠壓改變
        assert_eq!(
            again.entry("word/document.xml").unwrap().decoded().unwrap(),
            b"<w:document/>"
        );
    }

    #[test]
    fn rejects_unsupported_method_and_encrypted() {
        let mut b = Builder::new(b"");
        b.entry(b"a.bin", METHOD_STORE, b"x", b"", b"", b"", b"", 0, 0);
        let src = b.finish(b"");
        // 手動把 central method 改成 14 (LZMA)
        let mut bad = src.clone();
        let cd_at = find_eocd(&bad).unwrap();
        let cd_start = rd_u32(&bad, cd_at + 16).unwrap() as usize;
        bad[cd_start + 10] = 14;
        bad[cd_start + 11] = 0;
        assert!(matches!(
            ZipBundle::open(&bad),
            Err(ZipError::UnsupportedMethod { .. })
        ));
        // 加密 flag
        let mut bad2 = src.clone();
        bad2[cd_start + 8] |= 0x01;
        assert!(matches!(
            ZipBundle::open(&bad2),
            Err(ZipError::Encrypted { .. })
        ));
    }

    #[test]
    fn zip64_entry_offset_round_trips_untouched() {
        // 手工鑄一個 zip64 archive：document.xml 的 central offset 走 zip64 extra。
        let plain = b"<w:document/>".to_vec();
        let data = deflate(&plain);
        let crc = crc_of(&plain);
        let mut buf: Vec<u8> = Vec::new();
        let offset = buf.len() as u64;
        push_u32(&mut buf, SIG_LOCAL);
        push_u16(&mut buf, 45);
        push_u16(&mut buf, 0);
        push_u16(&mut buf, METHOD_DEFLATE);
        push_u16(&mut buf, 0x1234);
        push_u16(&mut buf, 0x5678);
        push_u32(&mut buf, crc);
        push_u32(&mut buf, fit32(data.len() as u64));
        push_u32(&mut buf, fit32(plain.len() as u64));
        push_u16(&mut buf, b"word/document.xml".len() as u16);
        push_u16(&mut buf, 0);
        buf.extend_from_slice(b"word/document.xml");
        buf.extend_from_slice(&data);
        let cd_off = buf.len() as u64;
        // central：三個欄位全打 marker＋zip64 extra(usize,csize,offset)
        let mut extra = Vec::new();
        push_zip64_extra(
            &mut extra,
            plain.len() as u64,
            data.len() as u64,
            Some(offset),
        );
        let mut c = Vec::new();
        push_u32(&mut c, SIG_CENTRAL);
        push_u16(&mut c, 0x031E);
        push_u16(&mut c, 45);
        push_u16(&mut c, 0);
        push_u16(&mut c, METHOD_DEFLATE);
        push_u16(&mut c, 0x1234);
        push_u16(&mut c, 0x5678);
        push_u32(&mut c, crc);
        push_u32(&mut c, ZIP64_MARKER);
        push_u32(&mut c, ZIP64_MARKER);
        push_u16(&mut c, b"word/document.xml".len() as u16);
        push_u16(&mut c, extra.len() as u16);
        push_u16(&mut c, 0);
        push_u16(&mut c, 0);
        push_u16(&mut c, 0);
        push_u32(&mut c, 0x01A40000);
        push_u32(&mut c, ZIP64_MARKER);
        c.extend_from_slice(b"word/document.xml");
        c.extend_from_slice(&extra);
        buf.extend_from_slice(&c);
        let cd_size = buf.len() as u64 - cd_off;
        // zip64 EOCD＋locator＋EOCD（全 marker）
        let z64_at = buf.len() as u64;
        let mut z64 = Vec::new();
        push_u32(&mut z64, SIG_EOCD64);
        push_u64(&mut z64, EOCD64_LEN as u64 - 12);
        push_u16(&mut z64, 45);
        push_u16(&mut z64, 45);
        push_u32(&mut z64, 0);
        push_u32(&mut z64, 0);
        push_u64(&mut z64, 1);
        push_u64(&mut z64, 1);
        push_u64(&mut z64, cd_size);
        push_u64(&mut z64, cd_off);
        buf.extend_from_slice(&z64);
        let mut loc = Vec::new();
        push_u32(&mut loc, SIG_EOCD64_LOC);
        push_u32(&mut loc, 0);
        push_u64(&mut loc, z64_at);
        push_u32(&mut loc, 1);
        buf.extend_from_slice(&loc);
        push_u32(&mut buf, SIG_EOCD);
        push_u16(&mut buf, 0);
        push_u16(&mut buf, 0);
        push_u16(&mut buf, u16::MAX);
        push_u16(&mut buf, u16::MAX);
        push_u32(&mut buf, ZIP64_MARKER);
        push_u32(&mut buf, ZIP64_MARKER);
        push_u16(&mut buf, 0);
        let src = buf;

        let bundle = ZipBundle::open(&src).expect("zip64 手工檔解析失敗");
        assert_eq!(bundle.entries.len(), 1);
        assert_eq!(bundle.entries[0].uncompressed_size, plain.len() as u64);
        let (ch, ex) = empty_changes();
        assert_eq!(
            bundle.serialize(&ch, &ex).unwrap(),
            src,
            "zip64 未觸碰必須位元組級相同"
        );
    }
}
