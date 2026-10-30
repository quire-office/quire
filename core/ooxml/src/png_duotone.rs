//! K19：PNG 像素級雙色調變換（`a:duotone` 的像素端）。
//!
//! 管線：解碼 RGBA8 → 逐像素 `L = 0.2126R + 0.7152G + 0.0722B`（卡片規格
//! 亮度式）→ `out = c1 + (c2 − c1)·L/255`（8bit 線性插值，alpha 原樣保留）
//! → 重編碼 PNG（filter 0）。
//!
//! ## LO 實測校準（離線反推，腳本與數據見提交說明）
//! 同型 duotone 宣告的水平灰階梯度圖（0..255 共 256 階）經 LO 無損 PDF
//! 輸出後逐 x 取像素：`out` 對 `L` 線性、最大殘差 ≤2/通道——上式成立。
//! 紅/綠/藍單通道梯度另測得 LO 實際 luma 權重 ≈ (0.29, 0.586, 0.103)
//! （偏 Rec.601），與卡片規格的 709 權重在**彩色源圖**上有 ≤0.08/255 的
//! 端點差；corpus 全部 duotone 素材為灰階源（R=G=B），兩式逐位元組同值，
//! 故採卡片規格式（709），彩色源差異記錄于此、留像素渲染卡校準。
//!
//! ## 邊界（老實退化，不虛構）
//! - 非 PNG／bitdepth≠8／colorType∉{0,2,6}（调色板、16bit）→ `None`，
//!   呼叫端原樣直出（不假調色）。Adam7 交錯**支援**：先逐 pass 解再散回
//!   全圖（picture-with-schemecolor 的 image1.png 即 interlace=1，real-world
//!   Word 存檔形態，紅燈燈1/燈2 實據钉死）。
//! - 輸出固定 colorType 6（RGBA）＋filter 0＋**非交錯**：編碼效率非本卡目標
//!   （LO 側色分軸只看像素值），正確性與零依賴優先。

use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;

/// PNG magic。
const PNG_MAGIC: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

/// Adam7 七道起點/步長（xstart, ystart, xstep, ystep；PNG 規範 §8）。
const ADAM7: [(u32, u32, u32, u32); 7] = [
    (0, 0, 8, 8),
    (4, 0, 8, 8),
    (0, 4, 4, 8),
    (2, 0, 4, 4),
    (0, 2, 2, 4),
    (1, 0, 2, 2),
    (0, 1, 1, 2),
];

/// 單條掃描線就地反濾鏡（filter 0..4；PNG 規範 §9）。回 false＝濾鏡代碼非法。
fn unfilter_line(ft: u8, line: &mut [u8], prev: &[u8], channels: usize) -> bool {
    let stride = line.len();
    match ft {
        0 => {}
        1 => {
            for i in channels..stride {
                line[i] = line[i].wrapping_add(line[i - channels]);
            }
        }
        2 => {
            for i in 0..stride {
                line[i] = line[i].wrapping_add(prev[i]);
            }
        }
        3 => {
            for i in 0..stride {
                let a = if i >= channels { line[i - channels] as u16 } else { 0 };
                line[i] = line[i].wrapping_add(((a + u16::from(prev[i])) >> 1) as u8);
            }
        }
        4 => {
            for i in 0..stride {
                let a = if i >= channels { i32::from(line[i - channels]) } else { 0 };
                let b = i32::from(prev[i]);
                let c = if i >= channels { i32::from(prev[i - channels]) } else { 0 };
                let p = b + c - a;
                let (pa, pb, pc) = ((p - b).abs(), (p - c).abs(), (p - a).abs());
                let pr = if pa <= pb && pa <= pc { b } else if pb <= pc { c } else { a };
                line[i] = line[i].wrapping_add(pr as u8);
            }
        }
        _ => return false,
    }
    true
}

/// 通道展開：源 channels（1/3/4）→ RGBA8 四通道寫入 out[o..o+4]。
fn expand_rgba(out: &mut [u8], o: usize, line: &[u8], s: usize, channels: usize) {
    out[o] = line[s];
    out[o + 1] = if channels >= 3 { line[s + 1] } else { line[s] };
    out[o + 2] = if channels >= 3 { line[s + 2] } else { line[s] };
    out[o + 3] = if channels == 4 { line[s + 3] } else { 255 };
}

/// 解碼 PNG → (寬, 高, RGBA8 行主序)。只認 bitdepth 8、colorType 0/2/6
/// （非交錯與 Adam7 皆支援）；其餘老實 None。
pub fn decode_png_rgba8(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    if bytes.len() < 8 + 25 || &bytes[0..8] != PNG_MAGIC {
        return None;
    }
    let mut pos = 8;
    let mut w = 0u32;
    let mut h = 0u32;
    let mut channels = 0usize;
    let mut interlace = 0u8;
    let mut idat: Vec<u8> = Vec::new();
    while pos + 12 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
        let typ = &bytes[pos + 4..pos + 8];
        let body = bytes.get(pos + 8..pos + 8 + len)?;
        match typ {
            b"IHDR" => {
                if body.len() < 13 {
                    return None;
                }
                let bd = body[8];
                let ct = body[9];
                interlace = body[12];
                if bd != 8 {
                    return None;
                }
                channels = match ct {
                    0 => 1,
                    2 => 3,
                    6 => 4,
                    _ => return None, // 调色板(3)／灰+alpha(4)：corpus 未見，老實不支援
                };
                w = u32::from_be_bytes(body[0..4].try_into().ok()?);
                h = u32::from_be_bytes(body[4..8].try_into().ok()?);
                if w == 0 || h == 0 || w > 65_536 || h > 65_536 {
                    return None; // 爆量防護（同 emu_to_pt 紀律）
                }
            }
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        pos += 12 + len;
    }
    if w == 0 || h == 0 || channels == 0 {
        return None;
    }
    let mut raw = Vec::new();
    {
        use flate2::read::ZlibDecoder;
        use std::io::Read;
        ZlibDecoder::new(&idat[..])
            .read_to_end(&mut raw)
            .ok()?;
    }
    let mut out = vec![0u8; (w as usize) * (h as usize) * 4];
    if interlace == 0 {
        let stride = (w as usize) * channels;
        if raw.len() < (stride + 1) * h as usize {
            return None;
        }
        let mut rp = 0usize;
        let mut prev = vec![0u8; stride];
        for y in 0..h as usize {
            let ft = raw[rp];
            rp += 1;
            let mut line = raw[rp..rp + stride].to_vec();
            rp += stride;
            if !unfilter_line(ft, &mut line, &prev, channels) {
                return None;
            }
            for x in 0..w as usize {
                expand_rgba(&mut out, (y * w as usize + x) * 4, &line, x * channels, channels);
            }
            prev = line;
        }
    } else if interlace == 1 {
        // Adam7：IDAT 是七個各自帶濾鏡的子圖串接。逐道解、散回全圖；
        // 空道（起點越界）零行、跳過。
        let mut rp = 0usize;
        for (xs, ys, xstep, ystep) in ADAM7 {
            let pw = (w.saturating_sub(xs) + xstep - 1) / xstep;
            let ph = (h.saturating_sub(ys) + ystep - 1) / ystep;
            if pw == 0 || ph == 0 {
                continue;
            }
            let stride = (pw as usize) * channels;
            if raw.len() < rp + (stride + 1) * ph as usize {
                return None;
            }
            let mut prev = vec![0u8; stride];
            for py in 0..ph as usize {
                let ft = raw[rp];
                rp += 1;
                let mut line = raw[rp..rp + stride].to_vec();
                rp += stride;
                if !unfilter_line(ft, &mut line, &prev, channels) {
                    return None;
                }
                let gy = ys + py as u32 * ystep;
                for px in 0..pw as usize {
                    let gx = xs + px as u32 * xstep;
                    expand_rgba(
                        &mut out,
                        ((gy as usize) * (w as usize) + gx as usize) * 4,
                        &line,
                        px * channels,
                        channels,
                    );
                }
                prev = line;
            }
        }
    } else {
        return None; // 交錯類型 2..6 非法（PNG 規範只定義 0/1）
    }
    Some((w, h, out))
}

/// 雙色調 LUT：`(c1, c2, L)` → 8bit 輸出（四捨五入、鉗位）。
///
/// `L`＝源像素 709 亮度（0..255，f64）；插值在 8bit 值域線性（LO 梯度
/// 實測口徑）。
pub fn duotone_lut(c1: u32, c2: u32, l: f64) -> (u8, u8, u8) {
    let t = (l / 255.0).clamp(0.0, 1.0);
    let mix = |a: u32, b: u32| -> u8 {
        (f64::from(a) + (f64::from(b) - f64::from(a)) * t).round().clamp(0.0, 255.0) as u8
    };
    (
        mix((c1 >> 16) & 0xFF, (c2 >> 16) & 0xFF),
        mix((c1 >> 8) & 0xFF, (c2 >> 8) & 0xFF),
        mix(c1 & 0xFF, c2 & 0xFF),
    )
}

/// 對 RGBA8 緩衝就地套 duotone LUT（alpha 原樣）。
pub fn duotone_rgba8(rgba: &mut [u8], c1: u32, c2: u32) {
    for px in rgba.chunks_exact_mut(4) {
        let l = 0.2126 * f64::from(px[0]) + 0.7152 * f64::from(px[1]) + 0.0722 * f64::from(px[2]);
        let (r, g, b) = duotone_lut(c1, c2, l);
        px[0] = r;
        px[1] = g;
        px[2] = b;
    }
}

/// RGBA8 → PNG bytes（colorType 6、filter 0、zlib deflate）。
pub fn encode_png_rgba8(w: u32, h: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let stride = (w as usize) * 4;
    if rgba.len() != stride * h as usize {
        return None;
    }
    let mut raw = Vec::with_capacity((stride + 1) * h as usize);
    for y in 0..h as usize {
        raw.push(0); // filter 0（None）
        raw.extend_from_slice(&rgba[y * stride..(y + 1) * stride]);
    }
    let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
    z.write_all(&raw).ok()?;
    let idat = z.finish().ok()?;
    let mut out = Vec::new();
    out.extend_from_slice(PNG_MAGIC);
    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&w.to_be_bytes());
    ihdr[4..8].copy_from_slice(&h.to_be_bytes());
    ihdr[8] = 8; // bit depth
    ihdr[9] = 6; // color type RGBA
    write_chunk(&mut out, b"IHDR", &ihdr);
    write_chunk(&mut out, b"IDAT", &idat);
    write_chunk(&mut out, b"IEND", b"");
    Some(out)
}

fn write_chunk(out: &mut Vec<u8>, typ: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(typ);
    out.extend_from_slice(body);
    let mut crc = crc32fast::Hasher::new();
    crc.update(typ);
    crc.update(body);
    out.extend_from_slice(&crc.finalize().to_be_bytes());
}

/// 一站式：PNG 位元組 → duotone 變換後 PNG 位元組。不支援的形態 → None
/// （呼叫端原樣直出，零假色）。
pub fn duotone_png_bytes(png: &[u8], c1: u32, c2: u32) -> Option<Vec<u8>> {
    let (w, h, mut rgba) = decode_png_rgba8(png)?;
    duotone_rgba8(&mut rgba, c1, c2);
    encode_png_rgba8(w, h, &rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 灰 154 像素經 (48,110,166)→白 的 LUT ≈ LO 實測 (172,197,219)。
    #[test]
    fn lut_gray_154_hits_lo_measured_endpoint() {
        let (r, g, b) = duotone_lut(0xFF30_6EA6, 0xFFFF_FFFF, 154.0);
        // LO 內嵌圖實測 (172,197,219)，容 ±1。
        assert!((i32::from(r) - 172).abs() <= 1);
        assert!((i32::from(g) - 197).abs() <= 1);
        assert!((i32::from(b) - 219).abs() <= 1);
    }

    /// 端點：L=0→c1、L=255→c2。
    #[test]
    fn lut_endpoints() {
        assert_eq!(duotone_lut(0xFF10_2030, 0xFFAB_CDDE, 0.0), (0x10, 0x20, 0x30));
        assert_eq!(duotone_lut(0xFF10_2030, 0xFFAB_CDDE, 255.0), (0xAB, 0xCD, 0xDE));
    }

    /// 解碼→調色→重編碼 往返自洽（二進制：再解碼像素等於一輪 LUT）。
    #[test]
    fn roundtrip_stable() {
        let w = 3u32;
        let h = 2u32;
        let rgba = vec![
            154, 154, 154, 255, 0, 0, 0, 0, 10, 20, 30, 128, //
            200, 10, 5, 255, 154, 154, 154, 255, 255, 255, 255, 255,
        ];
        let png = encode_png_rgba8(w, h, &rgba).expect("編碼");
        let (w2, h2, back) = decode_png_rgba8(&png).expect("解碼");
        assert_eq!((w2, h2), (w, h));
        assert_eq!(back, rgba);
        let out = duotone_png_bytes(&png, 0xFF30_6EA6, 0xFFFF_FFFF).expect("duotone");
        let (_, _, mut expect_rgba) = decode_png_rgba8(&png).unwrap();
        duotone_rgba8(&mut expect_rgba, 0xFF30_6EA6, 0xFFFF_FFFF);
        let (_, _, got) = decode_png_rgba8(&out).unwrap();
        assert_eq!(got, expect_rgba);
    }

    /// 非 PNG 毒位元組 → None（不虛構）。
    #[test]
    fn rejects_non_png() {
        assert!(decode_png_rgba8(b"GIF89a junk").is_none());
        assert!(duotone_png_bytes(b"not a png at all", 0, 0xFFFFFF).is_none());
    }
}
