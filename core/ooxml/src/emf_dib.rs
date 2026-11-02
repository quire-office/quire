//! K27：EMF 內嵌 DIB 直讀（P5 處方**路線 (b)**——不寫 EMF 向量解碼器）。
//!
//! ## 定位（誠實近似，不虛構）
//! EMF（MS-EMF）是記錄流容器。本模組**只**抽 `EMR_STRETCHDIBITS` 內嵌的未壓縮
//! DIB 當點陣圖用；EMF 的向量／文字記錄一概不解（那是路線 (a)，15–20 人日，
//! P5-diag §5 已否決）。因此本模組的成立條件是「這張 EMF 的內容**就是**那張
//! DIB」——條件不成立時老實回 `None`，呼叫端退既有灰色佔位框（K4c 口徑），
//! 絕不把「內容不全的圖」畫成「完整的圖」。
//!
//! ## 記錄流佈局：兩式並試（實測得，不是抄規格）
//! `EMR_STRETCHDIBITS` 固定區長度在不同寫入器不同：
//! - **MS-EMF §2.3.3.3 規格式**：固定區 96 位元組，`offBmi`／`cbBmi`／`offBits`／
//!   `cbBits` 在記錄內位移 **64／68／72／76**（`iUsage`＋`iRopColorUse` 佔 56..64）。
//! - **LibreOffice 實寫式**（corpus 三檔 media EMF 全部為此形態，本次逐記錄
//!   hex dump 覆核；`scripts/p5_emf_probe.py` 亦以同組位移讀到自洽值）：固定區
//!   80 位元組，同四個欄位在位移 **48／52／56／60**。
//!
//! 解析器**兩式都試**，以自我一致性判準取哪一式（[`RecordLayout::parse`]）：
//! `offBmi >= 固定區尾` ∧ `offBmi + cbBmi == offBits` ∧ `offBits + cbBits <= 記錄長`
//! ∧ `biSize >= 40` ∧ `planes == 1` ∧ `bitCount ∈ {1,4,8,16,24,32}` ∧
//! `cbBits >= stride·|height|`。三檔 corpus EMF 只有 48 式通過（64 式讀出的
//! `biSize` 是像素垃圾值）；純規格式寫入器只有 96 式通過——判準把兩者分開，
//! 不靠猜測。
//!
//! ## 成立條件（三道閘，逐條有實據；任一不過 ⇒ None）
//! 1. **無向量繪圖記錄**（[`vector_paint`]）：路徑／多邊形／圓弧／区域繪製系記錄
//!    一旦出現，代表 EMF 還畫了 DIB 之外的東西，只貼 DIB 就是虛構。
//!    實據：`n779834/image1.emf` 182 筆記錄含 `BEGINPATH×16`、`ENDPATH×16`、
//!    `FILLPATH×11`、`MOVETOEX×21`、`POLYBEZIERTO16×16`、`POLYPOLYGON16×6`——
//!    LO 對該檔的 1.87% 彩色 fill **全部**來自這些向量路徑（P5-diag §2-1），
//!    故必退佔位（該檔的 DIB 其實是 282×125 **24bpp 灰階**，卡面「無 DIB」的
//!    前提與實測不符：它**有** DIB，但 DIB 不是它的內容主體）。
//!    - **`EMR_BITBLT`/`EMR_STRETCHBLT` 不列否決項**：實據顯示它們在此三檔與
//!      `STRETCHDIBITS` **同目標矩形**（image1 `rclBounds=(5167,100,5867,875)`
//!      ＝ STRETCHDIBITS dest 逐位相同、`ROP=0xCC0020` SRCCOPY），是 LO 的
//!      雙緩衝複製而非獨立繪圖內容。
//!    - **`EXTTEXTOUT*` 不列否決項**：`fdo76591/image2.emf` 帶一筆 `EXTTEXTOUTW`
//!      仍是點陣主體（該圖自身彩色覆蓋 0.30%，P5-diag §2-2）；純文字型 EMF
//!      （全語料 13 parts）根本沒有 `STRETCHDIBITS`，天然不受本閘影響。
//! 2. **DIB 就是整張圖**（長寬比閘）：`rclFrame` 長寬比與 DIB 長寬比必須吻合
//!    （0.8..1.25）。實據：`fdo76591` 兩檔 = 0.993／1.002（DIB 填滿畫面框）；
//!    `n779834` = 0.301（畫面框 706.0×1039.1pt、DIB 只是 282×125px 的一小塊）
//!    ——畫面框遠大於 DIB 即代表框內另有內容。
//! 3. **可直讀的像素格式**：`BI_RGB`（16bpp＝X1R5G5B5）或 `BI_BITFIELDS`
//!    （16bpp 5-6-5 系遮罩）；24／32bpp 一併支援。`BI_PNG`／`BI_JPEG`／RLE／
//!    调色板（`biClrUsed>0` 或 bpp≤8）一律 `None`（老實退化：全語料 media EMF
//!    的 3 筆 DIB 皆 `BI_RGB`/`biClrUsed=0`，未實測分支不虛構）。ROP 必須是
//!    `SRCCOPY 0xCC0020`——其他光柵運算我們無法重現。
//!
//! ## 尺寸口徑：`rclFrame`，不是宿主 shape box
//! `rclFrame`（檔頭位移 24，單位 0.01mm）換算 pt（×72/2540）＝ LO 實際繪製尺寸。
//! P5-diag §3-2 實測我方 VML box 比 LO 大 7~30%（色軸 0.651 vs 0.680 的差就在
//! 這裡），故呼叫端發射時一律取本模組回傳的 `frame_pt`。置中對位同樣有實據：
//! image1 以 VML box 置中 rclFrame ⇒ (264.01, 594.03) vs LO 實測 (264.0, 594.0)。

use crate::png_duotone::encode_png_rgba8;

/// `EMR_HEADER`（MS-EMF §2.3.3.16）。
const EMR_HEADER: u32 = 1;
/// `EMR_STRETCHDIBITS`（MS-EMF §2.3.3.31）。
const EMR_STRETCHDIBITS: u32 = 81;
/// `BI_RGB`（MS-EMF §2.1.14）。
const BI_RGB: u32 = 0;
/// `BI_BITFIELDS`（MS-EMF §2.1.14）。
const BI_BITFIELDS: u32 = 3;
/// `SRCCOPY`（MS-EMF §2.1.23）——唯一我們能忠實重現的光柵運算。
const SRCCOPY: u32 = 0x00CC_0020;
/// 像素上限（防畸形檔 OOM）：16M 像素＝64MB RGBA8。corpus 最大 DIB 為 750×122。
const MAX_PIXELS: u64 = 16_000_000;

/// 抽出來的內嵌 DIB（top-down RGBA8）＋ `rclFrame` 換算尺寸。
#[derive(Debug, Clone, PartialEq)]
pub struct EmfDib {
    /// DIB 寬（像素）。
    pub width: u32,
    /// DIB 高（像素，已取絕對值）。
    pub height: u32,
    /// top-down RGBA8 像素（`width·height·4` 位元組）。
    pub rgba: Vec<u8>,
    /// EMF 檔頭 `rclFrame` 換算的 pt 尺寸（0.01mm×72/2540）。
    /// 檔頭未宣告（0×0）時為 `[0.0, 0.0]`——呼叫端退回宿主框尺寸。
    pub frame_pt: [f32; 2],
}

/// 拒絕理由（診斷／紅燈用；呼叫端只在乎是不是 `None`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmfReject {
    /// 檔頭非法（`iType != EMR_HEADER` 或長度不足）。
    BadHeader,
    /// 記錄流斷裂（首筆就切不出來）。
    BrokenRecords,
    /// 無 `EMR_STRETCHDIBITS`（純 vector／文字 EMF）。
    NoStretchDibits,
    /// 有向量繪圖記錄（閘 1）。
    VectorPaint,
    /// `rclFrame`／DIB 長寬比不合（閘 2）。
    AspectMismatch,
    /// 格式不可直讀（閘 3：BI_PNG/BI_JPEG/RLE/调色板/未知 bpp）。
    UnsupportedFormat,
    /// ROP 非 SRCCOPY。
    UnsupportedRop,
    /// 像素區長度不足／越界。
    TruncatedBits,
}

/// 抽內嵌 DIB；失敗回 `None`（呼叫端退佔位框）。
pub fn emf_embedded_dib(emf: &[u8]) -> Option<EmfDib> {
    emf_embedded_dib_checked(emf).ok()
}

/// 同 [`emf_embedded_dib`]，但回傳拒絕理由（紅燈與診斷用）。
///
/// 只取**首筆**通過三道閘的 `EMR_STRETCHDIBITS`（corpus 三檔皆恰一筆；多筆
/// 者代表 EMF 在畫多層內容，第一筆之外我們一概不負責——同閘 1 的誠實口徑）。
pub fn emf_embedded_dib_checked(emf: &[u8]) -> Result<EmfDib, EmfReject> {
    let hdr = Header::parse(emf).ok_or(EmfReject::BadHeader)?;
    let recs = records(emf, hdr.record_start).ok_or(EmfReject::BrokenRecords)?;
    // 閘 1 先行：整張 EMF 有任何向量繪圖記錄 ⇒ 一票否決（不看 DIB 好抽與否）。
    if recs.iter().any(|r| vector_paint(r.r#type)) {
        return Err(EmfReject::VectorPaint);
    }
    let mut saw_dib = false;
    for rec in recs.iter().filter(|r| r.r#type == EMR_STRETCHDIBITS) {
        saw_dib = true;
        let body = emf
            .get(rec.off..rec.off + rec.size)
            .ok_or(EmfReject::TruncatedBits)?;
        let Some(layout) = RecordLayout::parse(body) else {
            continue; // 本筆不可直讀：不虛構，也不就此否決全檔（續找下一筆）
        };
        if layout.rop != SRCCOPY {
            return Err(EmfReject::UnsupportedRop);
        }
        let Some(dib) = decode_dib(body, &layout) else {
            continue;
        };
        // 閘 2：DIB 必須就是整張畫面。
        if !aspect_ok(hdr.frame_pt, dib.width, dib.height) {
            return Err(EmfReject::AspectMismatch);
        }
        return Ok(EmfDib {
            width: dib.width,
            height: dib.height,
            rgba: dib.rgba,
            frame_pt: hdr.frame_pt,
        });
    }
    if saw_dib {
        Err(EmfReject::UnsupportedFormat)
    } else {
        Err(EmfReject::NoStretchDibits)
    }
}

/// 一站式：EMF 位元組 →（PNG 位元組，`rclFrame` pt 尺寸）。不可直讀 ⇒ `None`。
///
/// 重編 PNG 而非直出 RGBA：`DrawItem::Image` 走 `data:<mime>;base64` 內嵌，
/// PNG 是發射層白名單格式（K4c）且 `k19_duotone_pixels` 只認 PNG——管線零特例、
/// 下游（SVG／色軸量測）零改動。
pub fn emf_dib_png(emf: &[u8]) -> Option<(Vec<u8>, [f32; 2])> {
    let dib = emf_embedded_dib(emf)?;
    let png = encode_png_rgba8(dib.width, dib.height, &dib.rgba)?;
    Some((png, dib.frame_pt))
}

// ───────────────────────── 記錄流 ─────────────────────────

/// EMF 檔頭我們需要的兩欄。
struct Header {
    /// 第一筆記錄的位移（`nSize`）。
    record_start: usize,
    /// `rclFrame`（位移 24，0.01mm）換算 pt。
    frame_pt: [f32; 2],
}

impl Header {
    fn parse(emf: &[u8]) -> Option<Self> {
        // MS-EMF 固定欄：iType@0、nSize@4、rclBounds@8、rclFrame@24（l,t,r,b）、
        // iSignature@40（' EMF'）、nBytes@48、nRecords@52。
        let i32_at =
            |o: usize| emf.get(o..o + 4).and_then(|b| Some(i32::from_le_bytes(b.try_into().ok()?)));
        let i_type = u32::from_le_bytes(emf.get(0..4)?.try_into().ok()?);
        if i_type != EMR_HEADER {
            return None;
        }
        let n_size = u32::from_le_bytes(emf.get(4..8)?.try_into().ok()?) as usize;
        let (l, t, r, b) = (i32_at(24)?, i32_at(28)?, i32_at(32)?, i32_at(36)?);
        // 0.01mm → pt：×72/2540（1pt = 0.3528mm = 35.278 個 0.01mm）。
        let frame_pt = [(r - l) as f32 * 72.0 / 2540.0, (b - t) as f32 * 72.0 / 2540.0];
        // nSize 不可信時退回規格最小檔頭 88——後續自我一致性校驗自然把關。
        let record_start = if (88..=emf.len()).contains(&n_size) {
            n_size
        } else {
            88
        };
        Some(Header {
            record_start,
            frame_pt,
        })
    }
}

/// 一筆記錄的 (位移, 型別, 長度)。
struct Record {
    off: usize,
    r#type: u32,
    size: usize,
}

/// 逐筆切記錄流。**不**要求整條鏈乾淨到檔尾（畸形尾只讓後續記錄停止，已切出
/// 的部分照用）；回 `None`＝首筆就切不出來。
fn records(emf: &[u8], start: usize) -> Option<Vec<Record>> {
    let mut out = Vec::new();
    let mut off = start;
    while off + 8 <= emf.len() {
        let r#type = u32::from_le_bytes(emf[off..off + 4].try_into().ok()?);
        let size = u32::from_le_bytes(emf[off + 4..off + 8].try_into().ok()?) as usize;
        if size < 8 || off + size > emf.len() {
            break;
        }
        out.push(Record { off, r#type, size });
        off += size;
    }
    (!out.is_empty()).then_some(out)
}

/// 閘 1：會畫出「DIB 之外內容」的記錄型別（MS-EMF §2.1.1）。
///
/// 納入：多邊形／曲線系（2..=8、85..=92 的 16 位元變體）、`SETPIXELV`(15)、
/// 圖元系（42..=47 橢圓/矩形/圓角矩形/弧/弦/扇）、53..=57（EXTFLOODFILL/LINETO/
/// ARCTO/POLYDRAW/SETARCDIRECTION）、路徑系（59..=68 含 `FILLPATH`/`STROKEPATH`/
/// `STROKEANDFILLPATH`）、区域繪製（71..=74）。
///
/// **不**納入（實據見檔頭 §成立條件 1）：`BITBLT`/`STRETCHBLT`/`MASKBLT`/`PLGBLT`
/// （76..=79，同矩形雙緩衝複製）、文字系（82/83/84/96/97）、state／物件系
/// （9..=12/17/18/21/22/25/26/28/29/30/31/32/35/36/37/38/39/40/48..=52/67/93/94/
/// 95/98/99/100）、`SETDIBITSTODEVICE`(80)（同為 DIB 直出語意）。
fn vector_paint(t: u32) -> bool {
    matches!(t,
        2..=8            // POLYBEZIER..POLYPOLYGON
        | 15             // SETPIXELV
        | 42..=47        // ELLIPSE..PIE
        | 53..=57        // EXTFLOODFILL/LINETO/ARCTO/POLYDRAW/SETARCDIRECTION
        | 59..=68        // BEGINPATH..ABORTPATH（填／描邊路徑）
        | 71..=74        // FILLRGN/FRAMERGN/INVERTRGN/PAINTRGN
        | 85..=92)       // 16 位元變體（POLYBEZIER16..POLYDRAW16）
}

/// 閘 2：DIB 是否填滿畫面框（0.8..1.25）。畫面框未宣告（0×0）時不否決——
/// 由閘 1 與格式閘獨立把關，呼叫端退回宿主框尺寸。
fn aspect_ok(frame_pt: [f32; 2], w: u32, h: u32) -> bool {
    let (fw, fh) = (frame_pt[0], frame_pt[1]);
    if fw <= 0.0 || fh <= 0.0 || w == 0 || h == 0 {
        return true;
    }
    let ratio = (fw / fh) / (w as f32 / h as f32);
    (0.8..=1.25).contains(&ratio)
}

// ───────────────────────── STRETCHDIBITS 固定區 ─────────────────────────

/// `EMR_STRETCHDIBITS` 固定區校驗後的欄位（位移皆相對於記錄起點）。
struct RecordLayout {
    /// `BITMAPINFO` 起點。
    bmi: usize,
    /// `BITMAPINFO` 長度。
    cb_bmi: usize,
    /// 像素區起點。
    bits: usize,
    /// 像素區宣告長度。
    cb_bits: usize,
    /// `iRopColorUse`（ROP）。
    rop: u32,
}

impl RecordLayout {
    /// 兩式並試：`(固定區尾, offBmi 位移, rop 位移)` ＝ `(80, 48, 32)`（LO 實寫，
    /// `iRopColorUse` 在 `iUsage` 前）與 `(96, 64, 60)`（MS-EMF 規格）。
    ///
    /// 判準＝自我一致性（見檔頭）。兩式都過時取先試的 LO 式——實務上不可能
    /// 同時自洽（另一式讀出的是像素垃圾），真遇同時自洽者留 LO 式即可。
    fn parse(body: &[u8]) -> Option<Self> {
        [(80usize, 48usize, 32usize), (96, 64, 60)]
            .into_iter()
            .find_map(|(fixed_end, o_off, rop_off)| {
                Self::try_layout(body, fixed_end, o_off, rop_off)
            })
    }

    fn try_layout(body: &[u8], fixed_end: usize, o_off: usize, rop_off: usize) -> Option<Self> {
        if body.len() < fixed_end.max(o_off + 16).max(rop_off + 4) {
            return None;
        }
        let rd = |s: usize| u32::from_le_bytes(body[s..s + 4].try_into().ok().unwrap());
        let (off_bmi, cb_bmi, off_bits, cb_bits) =
            (rd(o_off), rd(o_off + 4), rd(o_off + 8), rd(o_off + 12));
        let rop = rd(rop_off);
        let (off_bmi_u, cb_bmi_u, off_bits_u) = (off_bmi as usize, cb_bmi as usize, off_bits as usize);
        if off_bmi_u < fixed_end
            || cb_bmi_u < 40
            || off_bmi_u.checked_add(cb_bmi_u)? != off_bits_u
            || off_bits_u.checked_add(cb_bits as usize)? > body.len()
        {
            return None;
        }
        // BIH 自我一致性：尺寸／ planes／ bpp 都要像話（垃圾欄位在此露餡）。
        let bmi = body.get(off_bmi_u..off_bmi_u + cb_bmi_u)?;
        let (bi_size, w, h, planes, bit_count, comp) = (
            u32::from_le_bytes(bmi[0..4].try_into().ok()?),
            i32::from_le_bytes(bmi[4..8].try_into().ok()?),
            i32::from_le_bytes(bmi[8..12].try_into().ok()?),
            u16::from_le_bytes(bmi[12..14].try_into().ok()?),
            u16::from_le_bytes(bmi[14..16].try_into().ok()?),
            u32::from_le_bytes(bmi[16..20].try_into().ok()?),
        );
        if bi_size < 40 || planes != 1 || w <= 0 || h == 0 {
            return None;
        }
        if !matches!(bit_count, 1 | 4 | 8 | 16 | 24 | 32) {
            return None;
        }
        let ah = h.unsigned_abs() as u64;
        let stride = (w as u64 * bit_count as u64 + 31) / 32 * 4;
        if (w as u64).saturating_mul(ah) > MAX_PIXELS {
            return None;
        }
        // 像素區裝得下宣告的位圖（BI_BITFIELDS 的遮罩前綴另計，由 decode 端校驗）。
        if (cb_bits as u64) < stride * ah {
            return None;
        }
        let _ = comp; // 壓縮型別由 decode_dib 判（此處只做幾何校驗）。
        Some(RecordLayout {
            bmi: off_bmi_u,
            cb_bmi: cb_bmi_u,
            bits: off_bits_u,
            cb_bits: cb_bits as usize,
            rop,
        })
    }
}

// ───────────────────────── DIB 解碼 ─────────────────────────

/// 16bpp 位元遮罩（R/G/B）。
type Masks = (u32, u32, u32);

/// 解 `BI_RGB`／`BI_BITFIELDS` 的 16/24/32bpp DIB → top-down RGBA8。
/// 其他（调色板、RLE、BI_PNG/BI_JPEG、1/4/8bpp）⇒ `None`（老實退化）。
fn decode_dib(body: &[u8], layout: &RecordLayout) -> Option<DibPixels> {
    let bmi = body.get(layout.bmi..layout.bmi + layout.cb_bmi)?;
    let bi_size = u32::from_le_bytes(bmi[0..4].try_into().ok()?) as usize;
    let (w, h, bit_count, comp) = (
        i32::from_le_bytes(bmi[4..8].try_into().ok()?) as usize,
        i32::from_le_bytes(bmi[8..12].try_into().ok()?),
        u16::from_le_bytes(bmi[14..16].try_into().ok()?) as usize,
        u32::from_le_bytes(bmi[16..20].try_into().ok()?),
    );
    // 调色板（bpp<=8 或 biClrUsed>0）：本卡不支援（實據：corpus 三筆 DIB 的
    // biClrUsed 皆 0、bpp>=16）——不虛構调色板展開。
    let clr_used = bmi
        .get(32..36)
        .and_then(|b| Some(u32::from_le_bytes(b.try_into().ok()?) as usize))
        .unwrap_or(0);
    if !matches!(bit_count, 16 | 24 | 32) || clr_used > 0 {
        return None;
    }
    let top_down = h < 0;
    let height = h.unsigned_abs();
    let masks = match comp {
        BI_RGB => None,
        BI_BITFIELDS => {
            // 遮罩位置兩式皆有實例：`biSize>40` 時在 BIH 內（BITMAPINFO 延伸，
            // cbBmi=52）；`biSize==40` 時在像素區開頭 12B（DIB 載體慣例）。
            if bi_size >= 52 && layout.cb_bmi >= 52 {
                Some((
                    u32::from_le_bytes(bmi[40..44].try_into().ok()?),
                    u32::from_le_bytes(bmi[44..48].try_into().ok()?),
                    u32::from_le_bytes(bmi[48..52].try_into().ok()?),
                ))
            } else {
                let b = body.get(layout.bits..layout.bits + 12)?;
                Some((
                    u32::from_le_bytes(b[0..4].try_into().ok()?),
                    u32::from_le_bytes(b[4..8].try_into().ok()?),
                    u32::from_le_bytes(b[8..12].try_into().ok()?),
                ))
            }
        }
        // BI_PNG(69)/BI_JPEG(70)/RLE 系：不支援（P5-diag §5 記錄為後續卡）。
        _ => return None,
    };
    let bpp_bytes = bit_count / 8;
    let stride = (w * bit_count + 31) / 32 * 4;
    // BI_BITFIELDS 且遮罩走像素區前綴時，cbBits 含那 12 位元組。
    let prefix = if comp == BI_BITFIELDS && bi_size < 52 { 12 } else { 0 };
    let need = stride.checked_mul(height as usize)?;
    let avail = body.len().saturating_sub(layout.bits);
    if avail < prefix + need || layout.cb_bits < prefix + need {
        return None;
    }
    let base = body.get(layout.bits + prefix..layout.bits + prefix + need)?;
    let mut rgba = vec![0u8; w * height as usize * 4];
    for y in 0..height as usize {
        // `biHeight>0`＝bottom-up（DIB 預設）；負高＝top-down。
        let sy = if top_down { y } else { height as usize - 1 - y };
        let row = &base[sy * stride..sy * stride + stride];
        let dst_y = y * w * 4;
        for x in 0..w {
            let src = x * bpp_bytes;
            let (r, g, b) = match bit_count {
                16 => {
                    let v = u16::from_le_bytes(row[src..src + 2].try_into().ok()?) as u32;
                    match masks {
                        Some(m) => expand16_masked(v, m),
                        None => expand1555(v),
                    }
                }
                // 24/32bpp：BGR(A)。32bpp 的第四位元組在 Windows DIB 是未定義
                // 保留（實務常為 0）⇒ 一律按不透明處理，不能照抄 alpha，
                // 否則整張圖變全透明。
                _ => (row[src + 2], row[src + 1], row[src]),
            };
            let d = dst_y + x * 4;
            rgba[d] = r;
            rgba[d + 1] = g;
            rgba[d + 2] = b;
            rgba[d + 3] = 255;
        }
    }
    Some(DibPixels {
        width: w as u32,
        height,
        rgba,
    })
}

struct DibPixels {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// 16bpp `BI_RGB`：X1R5G5B5（5-5-5）展開到 8 位元（`v·255/31`，與 P5 探針
/// `*255//31` 同口徑；5 位元滿值 31 → 255 恆等）。
fn expand1555(v: u32) -> (u8, u8, u8) {
    (
        (((v >> 10) & 31) * 255 / 31) as u8,
        (((v >> 5) & 31) * 255 / 31) as u8,
        ((v & 31) * 255 / 31) as u8,
    )
}

/// `BI_BITFIELDS`：依遮罩取位、按遮罩位元數標到 8 位元（5bit→×255/31、
/// 6bit→×255/63、8bit→原值）。X 位元（遮罩外高位）自然丟棄。
fn expand16_masked(v: u32, (rm, gm, bm): Masks) -> (u8, u8, u8) {
    let f = |m: u32| -> u8 {
        if m == 0 {
            return 0;
        }
        let shift = m.trailing_zeros();
        let bits = (m >> shift).count_ones();
        let raw = (v & m) >> shift;
        if bits >= 8 {
            raw.min(255) as u8
        } else {
            ((raw * 255) / ((1u32 << bits) - 1)) as u8
        }
    };
    (f(rm), f(gm), f(bm))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 老實寫一筆記錄（iType/iSize/payload）。
    fn record(t: u32, payload: &[u8]) -> Vec<u8> {
        let mut r = Vec::with_capacity(8 + payload.len());
        r.extend_from_slice(&t.to_le_bytes());
        r.extend_from_slice(&((8 + payload.len()) as u32).to_le_bytes());
        r.extend_from_slice(payload);
        r
    }

    /// 造 `EMR_STRETCHDIBITS`：`spec_layout` 選固定區式（false=LO 80B／true=MS 96B）。
    /// `mask_in_bits`＝BI_BITFIELDS 遮罩放像素區前綴（true）或 BIH 延伸（false）。
    fn stretch_dib(
        spec_layout: bool,
        w: i32,
        h: i32,
        bpp: u16,
        comp: u32,
        masks: Option<Masks>,
        bits: &[u8],
        rop: u32,
        mask_in_bits: bool,
    ) -> Vec<u8> {
        let fixed_end: usize = if spec_layout { 96 } else { 80 };
        let bih_len = if masks.is_some() && !mask_in_bits { 52 } else { 40 };
        let mut body = vec![0u8; fixed_end];
        // 兩式共同欄位（xDest/yDest/cxDest/cyDest 同在 24..40，rclBounds 8..24）。
        body[8..12].copy_from_slice(&0i32.to_le_bytes());
        body[12..16].copy_from_slice(&0i32.to_le_bytes());
        body[16..20].copy_from_slice(&w.to_le_bytes());
        body[20..24].copy_from_slice(&h.abs().to_le_bytes());
        body[24..28].copy_from_slice(&0i32.to_le_bytes());
        body[28..32].copy_from_slice(&0i32.to_le_bytes());
        body[32..36].copy_from_slice(&w.to_le_bytes());
        body[36..40].copy_from_slice(&h.abs().to_le_bytes());
        body[40..44].copy_from_slice(&w.to_le_bytes());
        body[44..48].copy_from_slice(&h.abs().to_le_bytes());

        let mut bih = vec![0u8; bih_len];
        bih[0..4].copy_from_slice(&(bih_len as u32).to_le_bytes());
        bih[4..8].copy_from_slice(&w.to_le_bytes());
        bih[8..12].copy_from_slice(&h.to_le_bytes());
        bih[12..14].copy_from_slice(&1u16.to_le_bytes());
        bih[14..16].copy_from_slice(&bpp.to_le_bytes());
        bih[16..20].copy_from_slice(&comp.to_le_bytes());
        if let Some((rm, gm, bm)) = masks {
            if !mask_in_bits {
                bih[40..44].copy_from_slice(&rm.to_le_bytes());
                bih[44..48].copy_from_slice(&gm.to_le_bytes());
                bih[48..52].copy_from_slice(&bm.to_le_bytes());
            }
        }
        let off_bmi = fixed_end;
        let off_bits = off_bmi + bih_len;
        let cb_bits = bits.len() + if mask_in_bits { 12 } else { 0 };
        let (o_off, rop_off) = if spec_layout { (64usize, 60usize) } else { (48, 32) };
        body[o_off..o_off + 4].copy_from_slice(&(off_bmi as u32).to_le_bytes());
        body[o_off + 4..o_off + 8].copy_from_slice(&(bih_len as u32).to_le_bytes());
        body[o_off + 8..o_off + 12].copy_from_slice(&(off_bits as u32).to_le_bytes());
        body[o_off + 12..o_off + 16].copy_from_slice(&(cb_bits as u32).to_le_bytes());
        body[rop_off..rop_off + 4].copy_from_slice(&rop.to_le_bytes());

        let mut payload = body;
        payload.extend_from_slice(&bih);
        if let Some((rm, gm, bm)) = masks {
            if mask_in_bits {
                payload.extend_from_slice(&rm.to_le_bytes());
                payload.extend_from_slice(&gm.to_le_bytes());
                payload.extend_from_slice(&bm.to_le_bytes());
            }
        }
        payload.extend_from_slice(bits);
        record(EMR_STRETCHDIBITS, &payload)
    }

    /// 88B 檔頭（rclFrame 起於 (0,0)）＋記錄串。
    fn wrap(frame_wh_01mm: (i32, i32), recs: &[Vec<u8>]) -> Vec<u8> {
        let n_size = 88usize;
        let total: usize = n_size + recs.iter().map(|r| r.len()).sum::<usize>();
        let mut out = vec![0u8; n_size];
        out[0..4].copy_from_slice(&1u32.to_le_bytes());
        out[4..8].copy_from_slice(&(n_size as u32).to_le_bytes());
        out[8..24].copy_from_slice(&[0u8; 16]);
        out[24..32].copy_from_slice(&0i32.to_le_bytes());
        out[32..36].copy_from_slice(&frame_wh_01mm.0.to_le_bytes());
        out[36..40].copy_from_slice(&frame_wh_01mm.1.to_le_bytes());
        out[40..44].copy_from_slice(b" EMF");
        out[48..52].copy_from_slice(&(total as u32).to_le_bytes());
        out[52..56].copy_from_slice(&((recs.len() + 1) as u32).to_le_bytes());
        for r in recs {
            out.extend_from_slice(r);
        }
        out
    }

    /// 2×2、16bpp、bottom-up：像素區第一列＝底列，第二列＝頂列。
    /// （stride＝(2×16+31)/32×4＝4 位元組＝恰兩像素，無需補齊。）
    fn dib16(rows_bottom_first: [[u16; 2]; 2]) -> Vec<u8> {
        let mut b = Vec::new();
        for row in rows_bottom_first {
            for px in row {
                b.extend_from_slice(&px.to_le_bytes());
            }
        }
        b
    }

    /// 2×2 5-5-5 樣本：頂列（0xF800 紅, 0x7FFF 白）、底列（0x001F 藍, 0x07C0 綠）。
    fn dib16_555() -> Vec<u8> {
        dib16([[0x001F, 0x07C0], [0xF800, 0x7FFF]])
    }

    #[test]
    fn lo_layout_16bpp_555_decodes_to_rgba() {
        // 卡面燈2 變體甲：16bpp 5-5-5（BI_RGB）＋ LO 實寫記錄式。
        let emf = wrap(
            (2000, 2000),
            &[stretch_dib(false, 2, 2, 16, BI_RGB, None, &dib16_555(), SRCCOPY, false)],
        );
        let d = emf_embedded_dib(&emf).expect("LO 式記錄必可抽 DIB");
        assert_eq!((d.width, d.height), (2, 2));
        // rclFrame 0.01mm → pt：2000×72/2540 = 56.69pt。
        assert!(
            (d.frame_pt[0] - 56.69).abs() < 0.02 && (d.frame_pt[1] - 56.69).abs() < 0.02,
            "{:?}",
            d.frame_pt
        );
        let px = |x: usize, y: usize| {
            let i = (y * 2 + x) * 4;
            (d.rgba[i], d.rgba[i + 1], d.rgba[i + 2], d.rgba[i + 3])
        };
        // bottom-up ⇒ 頂列是像素區第二列。
        assert_eq!(px(0, 0), (248, 0, 0, 255), "0xF800 5-5-5 紅");
        assert_eq!(px(1, 0), (255, 255, 255, 255), "0x7FFF ⇒ 全白");
        assert_eq!(px(0, 1), (0, 0, 248, 255), "0x001F ⇒ 藍");
        assert_eq!(px(1, 1), (0, 248, 0, 255), "0x07C0 ⇒ 綠");
    }

    #[test]
    fn spec_layout_16bpp_565_bitfields_decodes() {
        // 卡面燈2 變體乙：16bpp 5-6-5（BI_BITFIELDS）＋ MS-EMF 規格式記錄。
        // 遮罩放 BIH 延伸（biSize=52）。
        let masks: Masks = (0xF800, 0x07E0, 0x001F);
        let bits = dib16([[0x001F, 0x07E0], [0xF800, 0xFFFF]]);
        let emf = wrap(
            (2000, 2000),
            &[stretch_dib(
                true, 2, 2, 16, BI_BITFIELDS,
                Some(masks), &bits, SRCCOPY, false,
            )],
        );
        let d = emf_embedded_dib(&emf).expect("MS-EMF 規格式記錄必可抽 DIB");
        let px = |x: usize, y: usize| {
            let i = (y * 2 + x) * 4;
            (d.rgba[i], d.rgba[i + 1], d.rgba[i + 2], d.rgba[i + 3])
        };
        assert_eq!(px(0, 0), (248, 0, 0, 255), "565 紅端（5 位元）");
        assert_eq!(px(1, 0), (255, 255, 255, 255), "0xFFFF 565 ⇒ 全白");
        assert_eq!(px(0, 1), (0, 0, 248, 255), "565 藍端（5 位元）");
        assert_eq!(px(1, 1), (0, 255, 0, 255), "565 綠端（6 位元 ⇒ 63→255）");
    }

    #[test]
    fn bitfields_masks_in_bits_prefix_decodes() {
        // 第三式：BI_BITFIELDS 且 biSize=40 ⇒ 遮罩在像素區前綴 12B。
        let masks: Masks = (0xF800, 0x07E0, 0x001F);
        let bits = dib16([[0x001F, 0x07E0], [0xF800, 0xFFFF]]);
        let emf = wrap(
            (2000, 2000),
            &[stretch_dib(
                false, 2, 2, 16, BI_BITFIELDS,
                Some(masks), &bits, SRCCOPY, true,
            )],
        );
        let d = emf_embedded_dib(&emf).expect("遮罩前綴式必可抽 DIB");
        let i = 0usize; // 頂列首元 = 0xF800
        assert_eq!((d.rgba[i], d.rgba[i + 1], d.rgba[i + 2]), (248, 0, 0));
    }

    #[test]
    fn top_down_negative_height_decodes() {
        // biHeight<0＝top-down：像素區第一列就是頂列。
        let bits = dib16([[0xF800, 0x7FFF], [0x001F, 0x07C0]]);
        let emf = wrap(
            (2000, 2000),
            &[stretch_dib(false, 2, -2, 16, BI_RGB, None, &bits, SRCCOPY, false)],
        );
        let d = emf_embedded_dib(&emf).expect("負高 top-down 必可抽");
        let i = 0usize;
        assert_eq!((d.rgba[i], d.rgba[i + 1], d.rgba[i + 2]), (248, 0, 0),
                   "top-down ⇒ 像素區首列即頂列（紅）");
    }

    #[test]
    fn vector_records_reject_whole_emf() {
        // 閘 1：同樣的 DIB，多了 BEGINPATH/FILLPATH ⇒ 整張否決。
        for t in [59u32, 60, 62, 63, 64, 3, 4, 43, 88, 91, 71, 15] {
            let emf = wrap(
                (2000, 2000),
                &[
                    record(t, &[0u8; 8]),
                    stretch_dib(false, 2, 2, 16, BI_RGB, None, &dib16_555(), SRCCOPY, false),
                ],
            );
            assert_eq!(
                emf_embedded_dib_checked(&emf),
                Err(EmfReject::VectorPaint),
                "型別 {t} 是向量繪圖記錄 ⇒ 只貼 DIB 就是虛構，必須退佔位框"
            );
        }
        // 反鎖：BITBLT／STRETCHBLT／EXTTEXTOUTW／state 記錄**不得**否決——
        // 實據：fdo76591 image1 含 BITBLT、image2 含 EXTTEXTOUTW，兩檔都該抽出 DIB。
        for t in [76u32, 77, 84, 98, 70, 33, 34, 37, 80, 9, 10, 11, 12, 21, 95] {
            let emf = wrap(
                (2000, 2000),
                &[
                    record(t, &[0u8; 16]),
                    stretch_dib(false, 2, 2, 16, BI_RGB, None, &dib16_555(), SRCCOPY, false),
                ],
            );
            assert!(
                emf_embedded_dib(&emf).is_some(),
                "型別 {t} 不是向量繪圖記錄，不得否決點陣抽取"
            );
        }
    }

    #[test]
    fn aspect_mismatch_rejects() {
        // 閘 2：畫面框 4:1、DIB 1:1 ⇒ DIB 只佔畫面一小塊 ⇒ 否決（n779834 形態）。
        let emf = wrap(
            (20000, 5000),
            &[stretch_dib(false, 2, 2, 16, BI_RGB, None, &dib16_555(), SRCCOPY, false)],
        );
        assert_eq!(emf_embedded_dib_checked(&emf), Err(EmfReject::AspectMismatch));
        // 畫面框未宣告（0×0）⇒ 不由閘 2 否決（呼叫端退回宿主框尺寸）。
        let emf0 = wrap(
            (0, 0),
            &[stretch_dib(false, 2, 2, 16, BI_RGB, None, &dib16_555(), SRCCOPY, false)],
        );
        assert!(emf_embedded_dib(&emf0).is_some(), "無 rclFrame 不得由閘 2 否決");
    }

    #[test]
    fn unsupported_formats_and_rop_reject() {
        // BI_RLE4/8(1,2)、BI_JPEG(69)、BI_PNG(69→70)：不支援（後續卡）。
        for comp in [1u32, 2, 4, 5, 69, 70, 0xFF] {
            let emf = wrap(
                (2000, 2000),
                &[stretch_dib(false, 2, 2, 16, comp, None, &dib16_555(), SRCCOPY, false)],
            );
            assert!(
                emf_embedded_dib(&emf).is_none(),
                "壓縮型別 {comp} 不得假解（P5-diag §5：BI_PNG/BI_JPEG 屬後續卡）"
            );
        }
        // 调色板（biClrUsed>0）⇒ 不支援。
        let mut rec = stretch_dib(false, 2, 2, 16, BI_RGB, None, &dib16_555(), SRCCOPY, false);
        // BIH 起點＝記錄 body 的 offBmi=80 ⇒ 記錄內位移 8+80=88；biClrUsed@+32。
        let clr_at = 8 + 80 + 32;
        rec[clr_at..clr_at + 4].copy_from_slice(&4u32.to_le_bytes());
        let emf = wrap((2000, 2000), &[rec]);
        assert!(emf_embedded_dib(&emf).is_none(), "调色板 DIB 不得假解");
        // ROP 非 SRCCOPY ⇒ 無法忠實重現。
        let emf = wrap(
            (2000, 2000),
            &[stretch_dib(false, 2, 2, 16, BI_RGB, None, &dib16_555(), 0x00EE_0086, false)],
        );
        assert_eq!(emf_embedded_dib_checked(&emf), Err(EmfReject::UnsupportedRop));
        // 像素區截斷 ⇒ None（不可 panic、不可虛構半張圖）。
        let mut emf = wrap(
            (2000, 2000),
            &[stretch_dib(false, 2, 2, 16, BI_RGB, None, &dib16_555(), SRCCOPY, false)],
        );
        let cut = emf.len() - 6;
        emf.truncate(cut);
        assert!(emf_embedded_dib(&emf).is_none());
    }

    #[test]
    fn no_dib_and_garbage_return_none_not_panic() {
        // 只有 state 記錄、無 STRETCHDIBITS（純 vector／文字 EMF）。
        let emf = wrap((1000, 1000), &[record(33, &[]), record(98, &[0; 4])]);
        assert_eq!(emf_embedded_dib_checked(&emf), Err(EmfReject::NoStretchDibits));
        // 垃圾／畸形位元組：一律 None，不可 panic。
        let mut junk: Vec<Vec<u8>> = vec![
            vec![],
            vec![0u8; 4],
            vec![1, 0, 0, 0],
            vec![1, 0, 0, 0, 88, 0, 0, 0],
            vec![1, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0],
            vec![0xFFu8; 4096],
            vec![0u8; 88],
            {
                let mut v = vec![0u8; 88];
                v[0..4].copy_from_slice(&1u32.to_le_bytes());
                v[4..8].copy_from_slice(&88u32.to_le_bytes());
                v.extend_from_slice(&[9u8, 0, 0, 0, 4, 0, 0, 0]); // 記錄長 < 8
                v
            },
            {
                let mut v = vec![0u8; 88];
                v[0..4].copy_from_slice(&1u32.to_le_bytes());
                v[4..8].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // nSize 越界
                v
            },
        ];
        junk.extend((1..64).map(|k| {
            let mut base = wrap(
                (2000, 2000),
                &[stretch_dib(false, 2, 2, 16, BI_RGB, None, &dib16_555(), SRCCOPY, false)],
            );
            base.truncate(base.len() - k);
            base
        }));
        for j in junk.iter() {
            // 畸形輸入的契約＝不 panic、不虛構（回 Err 即可，理由不在此斷言）。
            assert!(
                emf_embedded_dib_checked(j).is_err(),
                "畸形輸入不得回傳僥倖解出的圖：len={}",
                j.len()
            );
        }
    }

    #[test]
    fn corpus_emfs_expected_outcomes() {
        // 覆核基準（本次逐記錄 hex dump 實測，也是 P5-diag §1-3 的再確認）。
        let docx = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/ooxmlexport/fdo76591.docx"
        ))
        .expect("corpus fdo76591");
        let doc = crate::parse(&docx).expect("parse fdo76591");
        let i1 = doc.parts.get("word/media/image1.emf").expect("image1.emf");
        let i2 = doc.parts.get("word/media/image2.emf").expect("image2.emf");
        let d1 = emf_embedded_dib(i1).expect("fdo76591 image1.emf 必可抽 DIB");
        assert_eq!((d1.width, d1.height), (166, 183), "DIB 尺寸（P5-diag §1-3）");
        assert!(
            (d1.frame_pt[0] - 83.93).abs() < 0.1 && (d1.frame_pt[1] - 93.15).abs() < 0.1,
            "rclFrame 換算 pt：{:?}",
            d1.frame_pt
        );
        let d2 = emf_embedded_dib(i2).expect("fdo76591 image2.emf 必可抽 DIB");
        assert_eq!((d2.width, d2.height), (750, 122));
        assert!(
            (d2.frame_pt[0] - 210.73).abs() < 0.1 && (d2.frame_pt[1] - 34.21).abs() < 0.1,
            "{:?}",
            d2.frame_pt
        );
        // 取證像素（Python 參考實作同點實測：0x5867→(180,24,57)、0x154E→(41,82,115)）。
        let at = |d: &EmfDib, x: usize, y: usize| {
            let i = (y * d.width as usize + x) * 4;
            (d.rgba[i], d.rgba[i + 1], d.rgba[i + 2], d.rgba[i + 3])
        };
        assert_eq!(at(&d1, 41, 45), (180, 24, 57, 255), "image1 取證彩色像素");
        assert_eq!(at(&d1, 124, 137), (41, 82, 115, 255), "image1 取證藍像素");
        assert_eq!(at(&d1, 0, 0), (255, 255, 255, 255), "image1 邊角白");
        // PNG 重編可用（發射層口徑）。
        let (png, fp) = emf_dib_png(i1).expect("PNG 重編必成");
        assert_eq!(&png[0..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(fp, d1.frame_pt);

        let n779 = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/ooxmlexport/n779834.docx"
        ))
        .expect("corpus n779834");
        let ndoc = crate::parse(&n779).expect("parse n779834");
        let em = ndoc.parts.get("word/media/image1.emf").expect("n779 image1.emf");
        assert_eq!(
            emf_embedded_dib_checked(em),
            Err(EmfReject::VectorPaint),
            "n779834 的 EMF **有** DIB（282×125 24bpp 灰階）但含 16 組路徑填色記錄 \
             ⇒ 必由閘 1 否決（卡面『無 DIB』的前提與實測不符，此鎖記錄真實理由）"
        );
    }
}
