//! S1 整合測試：ZIP 層不可信輸入加固。
//! 包含：解壓上限（單檔與總量炸彈）、CRC32 與宣告大小驗證、畸形 ZIP64 防護。

use flate2::write::DeflateEncoder;
use flate2::Compression;
use quire_ooxml::{
    parse, OoxmlError, ZipBundle, ZipError, MAX_ENTRY_DECOMPRESSED_SIZE,
    MAX_TOTAL_DECOMPRESSED_SIZE, METHOD_DEFLATE, METHOD_STORE,
};
use std::io::Write as _;

const SIG_LOCAL: u32 = 0x0403_4b50;
const SIG_CENTRAL: u32 = 0x0201_4b50;
const SIG_EOCD: u32 = 0x0605_4b50;
const SIG_EOCD64: u32 = 0x0606_4b50;
const SIG_EOCD64_LOC: u32 = 0x0706_4b50;
const XID_ZIP64: u16 = 0x0001;
const ZIP64_MARKER: u32 = u32::MAX;

struct ZipBuilder {
    buf: Vec<u8>,
    centrals: Vec<Vec<u8>>,
    count: usize,
}

impl ZipBuilder {
    fn new() -> Self {
        Self {
            buf: Vec::new(),
            centrals: Vec::new(),
            count: 0,
        }
    }

    fn add_raw_entry(
        &mut self,
        name: &[u8],
        method: u16,
        compressed_data: &[u8],
        crc: u32,
        uncompressed_size: u64,
        extra_central: &[u8],
        override_local_header_offset: Option<u64>,
    ) {
        let offset = override_local_header_offset.unwrap_or(self.buf.len() as u64);
        let csize = compressed_data.len() as u64;

        // Local header
        self.buf.extend_from_slice(&SIG_LOCAL.to_le_bytes());
        self.buf.extend_from_slice(&20u16.to_le_bytes()); // version needed
        self.buf.extend_from_slice(&0u16.to_le_bytes()); // flags
        self.buf.extend_from_slice(&method.to_le_bytes());
        self.buf.extend_from_slice(&0x1234u16.to_le_bytes()); // time
        self.buf.extend_from_slice(&0x5678u16.to_le_bytes()); // date
        self.buf.extend_from_slice(&crc.to_le_bytes());
        let csize32 = if csize > u32::MAX as u64 {
            ZIP64_MARKER
        } else {
            csize as u32
        };
        let usize32 = if uncompressed_size > u32::MAX as u64 {
            ZIP64_MARKER
        } else {
            uncompressed_size as u32
        };
        self.buf.extend_from_slice(&csize32.to_le_bytes());
        self.buf.extend_from_slice(&usize32.to_le_bytes());
        self.buf
            .extend_from_slice(&(name.len() as u16).to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes()); // extra len
        self.buf.extend_from_slice(name);
        self.buf.extend_from_slice(compressed_data);

        // Central directory entry
        let mut c = Vec::new();
        c.extend_from_slice(&SIG_CENTRAL.to_le_bytes());
        c.extend_from_slice(&0x031Eu16.to_le_bytes()); // version made by
        c.extend_from_slice(&20u16.to_le_bytes()); // version needed
        c.extend_from_slice(&0u16.to_le_bytes()); // flags
        c.extend_from_slice(&method.to_le_bytes());
        c.extend_from_slice(&0x1234u16.to_le_bytes()); // time
        c.extend_from_slice(&0x5678u16.to_le_bytes()); // date
        c.extend_from_slice(&crc.to_le_bytes());
        c.extend_from_slice(&csize32.to_le_bytes());
        c.extend_from_slice(&usize32.to_le_bytes());
        c.extend_from_slice(&(name.len() as u16).to_le_bytes());
        c.extend_from_slice(&(extra_central.len() as u16).to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes()); // comment len
        c.extend_from_slice(&0u16.to_le_bytes()); // disk
        c.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        c.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        let off32 = if offset > u32::MAX as u64 {
            ZIP64_MARKER
        } else {
            offset as u32
        };
        c.extend_from_slice(&off32.to_le_bytes());
        c.extend_from_slice(name);
        c.extend_from_slice(extra_central);
        self.centrals.push(c);
        self.count += 1;
    }

    fn finish(&self) -> Vec<u8> {
        let mut out = self.buf.clone();
        let cd_off = out.len() as u64;
        for c in &self.centrals {
            out.extend_from_slice(c);
        }
        let cd_size = out.len() as u64 - cd_off;

        out.extend_from_slice(&SIG_EOCD.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // disk
        out.extend_from_slice(&0u16.to_le_bytes()); // cd start disk
        out.extend_from_slice(&(self.count as u16).to_le_bytes());
        out.extend_from_slice(&(self.count as u16).to_le_bytes());
        out.extend_from_slice(&(cd_size as u32).to_le_bytes());
        out.extend_from_slice(&(cd_off as u32).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment len
        out
    }

    fn finish_with_zip64(
        &self,
        override_cd_off: Option<u64>,
        override_z64_at: Option<u64>,
    ) -> Vec<u8> {
        let mut out = self.buf.clone();
        let cd_off = out.len() as u64;
        for c in &self.centrals {
            out.extend_from_slice(c);
        }
        let cd_size = out.len() as u64 - cd_off;

        let z64_at = out.len() as u64;
        let mut z64 = Vec::new();
        z64.extend_from_slice(&SIG_EOCD64.to_le_bytes());
        z64.extend_from_slice(&(44u64).to_le_bytes()); // EOCD64 size
        z64.extend_from_slice(&45u16.to_le_bytes()); // version made by
        z64.extend_from_slice(&45u16.to_le_bytes()); // version needed
        z64.extend_from_slice(&0u32.to_le_bytes()); // disk
        z64.extend_from_slice(&0u32.to_le_bytes()); // cd disk
        z64.extend_from_slice(&(self.count as u64).to_le_bytes());
        z64.extend_from_slice(&(self.count as u64).to_le_bytes());
        z64.extend_from_slice(&cd_size.to_le_bytes());
        let effective_cd_off = override_cd_off.unwrap_or(cd_off);
        z64.extend_from_slice(&effective_cd_off.to_le_bytes());
        out.extend_from_slice(&z64);

        let mut loc = Vec::new();
        loc.extend_from_slice(&SIG_EOCD64_LOC.to_le_bytes());
        loc.extend_from_slice(&0u32.to_le_bytes()); // disk
        let effective_z64_at = override_z64_at.unwrap_or(z64_at);
        loc.extend_from_slice(&effective_z64_at.to_le_bytes());
        loc.extend_from_slice(&1u32.to_le_bytes()); // total disks
        out.extend_from_slice(&loc);

        // Standard EOCD with markers
        out.extend_from_slice(&SIG_EOCD.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&u16::MAX.to_le_bytes());
        out.extend_from_slice(&u16::MAX.to_le_bytes());
        out.extend_from_slice(&ZIP64_MARKER.to_le_bytes());
        out.extend_from_slice(&ZIP64_MARKER.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(data);
    h.finalize()
}

fn deflate_bytes(plain: &[u8]) -> Vec<u8> {
    let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
    enc.write_all(plain).unwrap();
    enc.finish().unwrap()
}

// ---------------------------------------------------------------- 1. 解壓上限測試

#[test]
fn test_single_entry_high_compression_bomb_rejected() {
    // 合成高壓縮比 entry：解壓後超過 MAX_ENTRY_DECOMPRESSED_SIZE (128 MiB)
    let bomb_plain_len = MAX_ENTRY_DECOMPRESSED_SIZE + 1024;
    let plain = vec![0u8; bomb_plain_len];
    let compressed = deflate_bytes(&plain);
    let crc = crc32(&plain);

    let mut b = ZipBuilder::new();
    b.add_raw_entry(
        b"word/document.xml",
        METHOD_DEFLATE,
        &compressed,
        crc,
        bomb_plain_len as u64,
        b"",
        None,
    );
    let zip_bytes = b.finish();

    let bundle = ZipBundle::open(&zip_bytes).expect("open 標頭成功");
    let entry = bundle.entry("word/document.xml").expect("entry 存在");

    // 斷言 decoded() 回傳 TooLarge
    let err = entry.decoded().expect_err("必須阻絕超過單一上限的炸彈");
    assert_eq!(
        err,
        ZipError::TooLarge {
            name: "word/document.xml".into(),
            limit: MAX_ENTRY_DECOMPRESSED_SIZE,
        }
    );

    // 斷言 decode_parts() 亦回傳 TooLarge
    let parts_err = bundle
        .decode_parts()
        .expect_err("decode_parts 必須阻絕超過單一上限的炸彈");
    assert_eq!(
        parts_err,
        ZipError::TooLarge {
            name: "word/document.xml".into(),
            limit: MAX_ENTRY_DECOMPRESSED_SIZE,
        }
    );

    // 斷言 parse() 透過 OoxmlError 轉播 TooLarge
    let parse_err = parse(&zip_bytes).expect_err("parse 必須阻絕炸彈");
    assert_eq!(
        parse_err,
        OoxmlError::Zip(ZipError::TooLarge {
            name: "word/document.xml".into(),
            limit: MAX_ENTRY_DECOMPRESSED_SIZE,
        })
    );
}

#[test]
fn test_total_volume_bomb_rejected() {
    // 總量炸彈：多個 entry 單一大小合法，但總和超過 MAX_TOTAL_DECOMPRESSED_SIZE (512 MiB)
    // 為了兼顧測試速度與規格要求，以 5 個 100 MiB 加上 1 個 20 MiB 實體資料實測：
    // 前 5 個 entry 各 100 MiB (小於 128 MiB 單一上限)，累計 500 MiB。
    // 第 6 個 entry 20 MiB，解壓累計 520 MiB > 512 MiB 上限。
    let chunk_size = 100 * 1024 * 1024;
    let chunk_plain = vec![0u8; chunk_size];
    let chunk_comp = deflate_bytes(&chunk_plain);
    let chunk_crc = crc32(&chunk_plain);

    let last_size = 20 * 1024 * 1024;
    let last_plain = vec![0u8; last_size];
    let last_comp = deflate_bytes(&last_plain);
    let last_crc = crc32(&last_plain);

    let mut b = ZipBuilder::new();
    for i in 0..5 {
        let name = format!("part{i}.xml");
        b.add_raw_entry(
            name.as_bytes(),
            METHOD_DEFLATE,
            &chunk_comp,
            chunk_crc,
            chunk_size as u64,
            b"",
            None,
        );
    }
    b.add_raw_entry(
        b"part_last.xml",
        METHOD_DEFLATE,
        &last_comp,
        last_crc,
        last_size as u64,
        b"",
        None,
    );
    let zip_bytes = b.finish();

    let bundle = ZipBundle::open(&zip_bytes).expect("open 成功");

    // 單一 entry 解壓均能成功（未超 128 MiB）
    let first = bundle.entry("part0.xml").expect("part0 存在");
    assert!(first.decoded().is_ok());

    // 整包總解壓 decode_parts 必須報 TooLarge
    let err = bundle
        .decode_parts()
        .expect_err("整包解壓超過總上限必須報 TooLarge");
    assert_eq!(
        err,
        ZipError::TooLarge {
            name: "part_last.xml".into(),
            limit: MAX_TOTAL_DECOMPRESSED_SIZE,
        }
    );
}

#[test]
fn test_total_volume_bomb_custom_limits() {
    // 以小量自訂上限驗證層級邏輯：單檔上限 1000 位元組，整包上限 2500 位元組
    let plain = vec![b'A'; 900];
    let comp = deflate_bytes(&plain);
    let crc = crc32(&plain);

    let mut b = ZipBuilder::new();
    for i in 0..3 {
        let name = format!("sub{i}.txt");
        b.add_raw_entry(
            name.as_bytes(),
            METHOD_DEFLATE,
            &comp,
            crc,
            plain.len() as u64,
            b"",
            None,
        );
    }
    let zip_bytes = b.finish();
    let bundle = ZipBundle::open(&zip_bytes).unwrap();

    let err = bundle
        .decode_parts_with_limits(1000, 2500)
        .expect_err("3 * 900 = 2700 超過總上限 2500");
    assert_eq!(
        err,
        ZipError::TooLarge {
            name: "sub2.txt".into(),
            limit: 2500,
        }
    );
}

// ---------------------------------------------------------------- 2. CRC32 與宣告大小驗證

#[test]
fn test_crc_mismatch_deflate_fails_with_corrupt() {
    let plain = b"<doc>valid content</doc>";
    let comp = deflate_bytes(plain);
    let fake_crc = crc32(plain) ^ 0xDEADBEEF;

    let mut b = ZipBuilder::new();
    b.add_raw_entry(
        b"doc.xml",
        METHOD_DEFLATE,
        &comp,
        fake_crc,
        plain.len() as u64,
        b"",
        None,
    );
    let zip_bytes = b.finish();
    let bundle = ZipBundle::open(&zip_bytes).unwrap();
    let entry = bundle.entry("doc.xml").unwrap();

    let err = entry.decoded().expect_err("CRC 不符必須拒絕");
    assert_eq!(err, ZipError::Corrupt("CRC-32 校驗不符"));
}

#[test]
fn test_crc_mismatch_store_fails_with_corrupt() {
    let plain = b"uncompressed data";
    let fake_crc = crc32(plain) ^ 0x12345678;

    let mut b = ZipBuilder::new();
    b.add_raw_entry(
        b"raw.bin",
        METHOD_STORE,
        plain,
        fake_crc,
        plain.len() as u64,
        b"",
        None,
    );
    let zip_bytes = b.finish();
    let bundle = ZipBundle::open(&zip_bytes).unwrap();
    let entry = bundle.entry("raw.bin").unwrap();

    let err = entry.decoded().expect_err("STORE CRC 不符必須拒絕");
    assert_eq!(err, ZipError::Corrupt("CRC-32 校驗不符"));
}

#[test]
fn test_declared_size_mismatch_deflate_fails_with_corrupt() {
    let plain = b"sample content";
    let comp = deflate_bytes(plain);
    let crc = crc32(plain);
    let fake_uncompressed_size = (plain.len() + 99) as u64;

    let mut b = ZipBuilder::new();
    b.add_raw_entry(
        b"doc.xml",
        METHOD_DEFLATE,
        &comp,
        crc,
        fake_uncompressed_size,
        b"",
        None,
    );
    let zip_bytes = b.finish();
    let bundle = ZipBundle::open(&zip_bytes).unwrap();
    let entry = bundle.entry("doc.xml").unwrap();

    let err = entry.decoded().expect_err("宣告大小不符必須拒絕");
    assert_eq!(err, ZipError::Corrupt("解壓大小與宣告不符"));
}

#[test]
fn test_declared_size_mismatch_store_fails_with_corrupt() {
    let plain = b"sample content";
    let crc = crc32(plain);
    let fake_uncompressed_size = (plain.len() + 42) as u64;

    let mut b = ZipBuilder::new();
    b.add_raw_entry(
        b"raw.bin",
        METHOD_STORE,
        plain,
        crc,
        fake_uncompressed_size,
        b"",
        None,
    );
    let zip_bytes = b.finish();
    let bundle = ZipBundle::open(&zip_bytes).unwrap();
    let entry = bundle.entry("raw.bin").unwrap();

    let err = entry.decoded().expect_err("STORE 宣告大小不符必須拒絕");
    assert_eq!(err, ZipError::Corrupt("解壓大小與宣告不符"));
}

// ---------------------------------------------------------------- 3. 畸形 ZIP64 防護（不 panic，報 Corrupt）

#[test]
fn test_malformed_zip64_cd_off_2_32_plus_100() {
    let plain = b"abc";
    let crc = crc32(plain);
    let mut b = ZipBuilder::new();
    b.add_raw_entry(
        b"a.txt",
        METHOD_STORE,
        plain,
        crc,
        plain.len() as u64,
        b"",
        None,
    );

    // cd_off = 2^32 + 100
    let bad_cd_off = (1u64 << 32) + 100;
    let bad_zip = b.finish_with_zip64(Some(bad_cd_off), None);

    let res = ZipBundle::open(&bad_zip);
    assert!(
        matches!(res, Err(ZipError::Corrupt(_))),
        "畸形 ZIP64 cd_off 不可 panic，必須回傳 Corrupt，實際為: {res:?}"
    );
}

#[test]
fn test_malformed_zip64_cd_off_u64_max() {
    let plain = b"abc";
    let crc = crc32(plain);
    let mut b = ZipBuilder::new();
    b.add_raw_entry(
        b"a.txt",
        METHOD_STORE,
        plain,
        crc,
        plain.len() as u64,
        b"",
        None,
    );

    let bad_zip = b.finish_with_zip64(Some(u64::MAX), None);
    let res = ZipBundle::open(&bad_zip);
    assert!(
        matches!(res, Err(ZipError::Corrupt(_))),
        "u64::MAX cd_off 不可 panic，必須回傳 Corrupt，實際為: {res:?}"
    );
}

#[test]
fn test_malformed_zip64_z64_at_over_bounds() {
    let plain = b"abc";
    let crc = crc32(plain);
    let mut b = ZipBuilder::new();
    b.add_raw_entry(
        b"a.txt",
        METHOD_STORE,
        plain,
        crc,
        plain.len() as u64,
        b"",
        None,
    );

    // z64_at = 2^32 + 100
    let bad_z64_at = (1u64 << 32) + 100;
    let bad_zip = b.finish_with_zip64(None, Some(bad_z64_at));
    let res = ZipBundle::open(&bad_zip);
    assert!(
        matches!(res, Err(ZipError::Corrupt(_))),
        "畸形 z64_at 不可 panic，必須回傳 Corrupt，實際為: {res:?}"
    );

    // z64_at = u64::MAX
    let bad_zip2 = b.finish_with_zip64(None, Some(u64::MAX));
    let res2 = ZipBundle::open(&bad_zip2);
    assert!(
        matches!(res2, Err(ZipError::Corrupt(_))),
        "u64::MAX z64_at 不可 panic，必須回傳 Corrupt，實際為: {res2:?}"
    );
}

#[test]
fn test_malformed_zip64_local_header_offset_overflow() {
    let plain = b"abc";
    let crc = crc32(plain);

    // central extra 帶 zip64 extra，其中 offset 為 2^32 + 100
    let bad_offset = (1u64 << 32) + 100;
    let mut extra = Vec::new();
    extra.extend_from_slice(&XID_ZIP64.to_le_bytes());
    extra.extend_from_slice(&(24u16).to_le_bytes());
    extra.extend_from_slice(&(plain.len() as u64).to_le_bytes()); // usize
    extra.extend_from_slice(&(plain.len() as u64).to_le_bytes()); // csize
    extra.extend_from_slice(&bad_offset.to_le_bytes()); // offset

    let mut b = ZipBuilder::new();
    b.add_raw_entry(
        b"a.txt",
        METHOD_STORE,
        plain,
        crc,
        ZIP64_MARKER as u64,
        &extra,
        Some(ZIP64_MARKER as u64),
    );
    let zip = b.finish();

    let res = ZipBundle::open(&zip);
    assert!(
        matches!(res, Err(ZipError::Corrupt(_))),
        "畸形 local_header_offset 不可 panic，必須回傳 Corrupt，實際為: {res:?}"
    );

    // local_header_offset = u64::MAX
    let mut extra_max = Vec::new();
    extra_max.extend_from_slice(&XID_ZIP64.to_le_bytes());
    extra_max.extend_from_slice(&(24u16).to_le_bytes());
    extra_max.extend_from_slice(&(plain.len() as u64).to_le_bytes());
    extra_max.extend_from_slice(&(plain.len() as u64).to_le_bytes());
    extra_max.extend_from_slice(&u64::MAX.to_le_bytes());

    let mut b2 = ZipBuilder::new();
    b2.add_raw_entry(
        b"a.txt",
        METHOD_STORE,
        plain,
        crc,
        ZIP64_MARKER as u64,
        &extra_max,
        Some(ZIP64_MARKER as u64),
    );
    let zip2 = b2.finish();

    let res2 = ZipBundle::open(&zip2);
    assert!(
        matches!(res2, Err(ZipError::Corrupt(_))),
        "u64::MAX local_header_offset 不可 panic，必須回傳 Corrupt，實際為: {res2:?}"
    );
}
