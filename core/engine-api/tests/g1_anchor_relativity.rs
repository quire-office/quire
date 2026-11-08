//! G1 紅燈考卷：`wp:anchor` 之 `positionV relativeFrom="paragraph"` 與
//! `positionH relativeFrom="column"` 原點換算、跨頁宿主列附掛頁、框線內距補償與多欄原點。
//!
//! 署名：ADT109119
//! 依據：`docs/g1-anchor-relativity-anatomy.md` 四落點（A/B/C/D）與 LO 真值律。
//!
//! - 燈 1（落點 B）：`g1_anchor_paging.docx` 錨定表第 1 列（含兩枚 `wp:anchor`）被第 0 列
//!   高墊列推至次頁時，兩枚浮動圖必須附掛在**宿主列實際落點頁**，不得滯留在表格起頭頁。
//! - 燈 2（落點 A ＋ C）：宿主列頂 y 必須取分頁後該列在頁面上的**實際渲染頂**（而非靜態
//!   `margin_top` 假設），且 `positionV relativeFrom="paragraph"`（`layoutInCell=0`）須加上
//!   宿主格上框線寬（2.0pt）＋上內距（2.75pt）＋posOffset（0.05pt）＝ 列頂 ＋ 4.80pt（±2pt）。
//!   當宿主表位於頁中（前有跨頁表續列或段落）時，同頁第 1 列頂為 522.0pt，圖頂 y 落於
//!   LO 真值 526.75 ± 2.0pt。
//! - 燈 3（落點 D）：`positionH relativeFrom="column"`（`layoutInCell=0`）原點須取儲存格
//!   **欄內容盒左緣**（表左 ＋ 格左 ＋ ½左框線寬 1.0pt ＋ 左內距 2.75pt），且 `<wp:align>center</wp:align>`
//!   須在內容盒寬（格寬 − 左內距 − 右內距）內置中：
//!   A 圖 x = 96.50 ± 2.0pt，B 圖 x = 304.10 ± 2.0pt，兩圖間距 = 207.60 ± 2.0pt。
//! - 燈 4（多欄 `w:cols` ＋ z 序）：非表格多欄節（`w:cols w:num="2"`）次欄段落的
//!   `positionH relativeFrom="column"` 錨點圖，x 原點須取次欄實際內容盒起點（315.65pt），
//!   不得偷渡版心左緣（72.0pt）；`behindDoc="0"` 浮於表格框線之上且依 `relativeHeight` 排序。
//! - 燈 5（`G1_BUGGY=1` 回塞鎖）：設 `G1_BUGGY=1` 時退回修前路徑，證明考卷具鑑別力。
//! - 用戶真實檔案考卷（`QUIRE_G1_FIXTURE`）：未設定環境變數時自動略過；設定時驗證 p1
//!   兩張並排照片不滯留版心頂 y=72.1pt。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

const FIXTURE_REL: &str = "tests/fixtures/g1_anchor_paging.docx";

/// LO 實測真值（`docs/g1-anchor-relativity-anatomy.md` §3）。
const LO_IMG_W: f32 = 202.15;
const LO_IMG_H: f32 = 113.70;
const LO_IMG_A_X: f32 = 96.50;
const LO_IMG_B_X: f32 = 304.10;
const LO_IMG_AB_GAP: f32 = 207.60;
const LO_ROW_OFFSET_Y: f32 = 4.75; // LO 526.75 - 標稱列頂 522.0 = 4.75（反解式 2.0 + 2.75 + 0.05 = 4.80）
const LO_SAME_PAGE_Y: f32 = 526.75;
const TOL_PT: f32 = 2.0;

fn load_g1_fixture() -> Engine {
    let full = format!("{}/{}", env!("CARGO_MANIFEST_DIR"), FIXTURE_REL);
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀取 {full} 失敗: {e}"));
    Engine::from_docx_bytes(&bytes).expect("解析 g1_anchor_paging.docx 應成功")
}

/// 找出指定頁面上尺寸匹配 G1 錨點圖（202.15×113.70pt ± 1.5pt）的所有 `DrawItem::Image` 矩形 (x, y, w, h)。
fn page_anchor_images(eng: &Engine, page: usize) -> Vec<(f32, f32, f32, f32)> {
    let Some(dl) = eng.page_display_list(page) else {
        return Vec::new();
    };
    dl.items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Image { x, y, w, h, .. }
                if (*w - LO_IMG_W).abs() < 1.5 && (*h - LO_IMG_H).abs() < 1.5 =>
            {
                Some((*x, *y, *w, *h))
            }
            _ => None,
        })
        .collect()
}

/// 找出含指定文字片段的頁碼。
fn find_page_with_text(eng: &Engine, needle: &str) -> Option<usize> {
    for p in 0..eng.page_count() {
        let dl = eng.page_display_list(p)?;
        let hit = dl.items.iter().any(|it| match it {
            DrawItem::Text { text, .. } => text.contains(needle),
            _ => false,
        });
        if hit {
            return Some(p);
        }
    }
    None
}

/// 找出指定頁面上錨定表（線寬 2.0pt、x1≈92.75..508.05）的橫向表格框線 y 序列（排序去重）。
fn anchor_table_h_rules(eng: &Engine, page: usize) -> Vec<f32> {
    let Some(dl) = eng.page_display_list(page) else {
        return Vec::new();
    };
    let mut ys: Vec<f32> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Rule {
                x1,
                y1,
                x2,
                y2,
                width_pt,
                ..
            } if (y1 - y2).abs() < 0.05
                && (*width_pt - 2.0).abs() < 0.2
                && *x1 >= 90.0
                && *x2 <= 512.0 =>
            {
                Some(*y1)
            }
            _ => None,
        })
        .collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.2);
    ys
}

/// 動態組裝最小 DOCX 位元組（純記憶體 zip，供中段續表與雙欄 `w:cols` 錨點測試用）。
fn build_minimal_docx(document_xml: &str) -> Vec<u8> {
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Default Extension="png" ContentType="image/png"/>
</Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rIdImgA" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/imageA.png"/>
<Relationship Id="rIdImgB" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/imageB.png"/>
</Relationships>"#;
    // 1x1 純色合法 PNG
    let png: &[u8] = &[
        0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, b'I', b'H', b'D',
        b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xde, 0x00, 0x00, 0x00, 0x0c, b'I', b'D', b'A', b'T', 0x08, 0xd7, 0x63, 0xf8,
        0xcf, 0xc0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xdd, 0x8d, 0xb0, 0x00, 0x00, 0x00,
        0x00, b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82,
    ];

    let mut out = Vec::new();
    let mut centrals = Vec::new();
    let entries: [(&str, &[u8]); 6] = [
        ("[Content_Types].xml", ct.as_bytes()),
        ("_rels/.rels", rels.as_bytes()),
        ("word/_rels/document.xml.rels", doc_rels.as_bytes()),
        ("word/document.xml", document_xml.as_bytes()),
        ("word/media/imageA.png", png),
        ("word/media/imageB.png", png),
    ];
    for (name, data) in entries {
        let offset = out.len() as u32;
        let csize = data.len() as u32;
        let mut crc = flate2::Crc::new();
        crc.update(data);
        let crc = crc.sum();

        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0x1234u16.to_le_bytes());
        out.extend_from_slice(&0x5678u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&csize.to_le_bytes());
        out.extend_from_slice(&csize.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);

        let mut c = Vec::new();
        c.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        c.extend_from_slice(&0x031Eu16.to_le_bytes());
        c.extend_from_slice(&20u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0x1234u16.to_le_bytes());
        c.extend_from_slice(&0x5678u16.to_le_bytes());
        c.extend_from_slice(&crc.to_le_bytes());
        c.extend_from_slice(&csize.to_le_bytes());
        c.extend_from_slice(&csize.to_le_bytes());
        c.extend_from_slice(&(name.len() as u16).to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&offset.to_le_bytes());
        c.extend_from_slice(name.as_bytes());
        centrals.push(c);
    }
    let cd_off = out.len() as u32;
    for c in &centrals {
        out.extend_from_slice(c);
    }
    let cd_size = out.len() as u32 - cd_off;
    let count = entries.len() as u16;
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_off.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// 燈 1（落點 B）：錨定列被首列高墊列推到次頁時，兩枚浮動圖必須掛在錨定列實際所在頁，
/// 不得滯留在錨定表起頭頁（首列所在頁）。
#[test]
fn g1_lamp1_anchor_attached_to_host_row_actual_page() {
    let eng = load_g1_fixture();
    let row0_page =
        find_page_with_text(&eng, "錨表首列墊高文字").expect("必須找到錨定表首列文字所在頁");
    let tail_page =
        find_page_with_text(&eng, "尾部段落 0").expect("必須找到錨定表之後的尾部段落所在頁");

    // 首列在 row0_page，而第 1 列（錨定列，trHeight=147.75pt）被推到次頁（tail_page）。
    assert!(
        tail_page > row0_page,
        "錨定表第 1 列應被 450pt 首列推至次頁：row0_page={row0_page}, tail_page={tail_page}"
    );
    let anchor_row_page = tail_page;

    let imgs_on_row0_page = page_anchor_images(&eng, row0_page);
    let imgs_on_anchor_page = page_anchor_images(&eng, anchor_row_page);

    assert!(
        imgs_on_row0_page.is_empty(),
        "燈1（落點B）：錨定表起頭頁 p{row0_page} 不得滯留第 1 列的浮動圖，實得: {imgs_on_row0_page:?}"
    );
    assert_eq!(
        imgs_on_anchor_page.len(),
        2,
        "燈1（落點B）：錨定列實際落點頁 p{anchor_row_page} 必須發射 2 枚浮動圖，實得: {imgs_on_anchor_page:?}"
    );
}

/// 燈 2（落點 A ＋ C）：V 軸 `positionV relativeFrom="paragraph"` 原點必須取宿主列在渲染頁上的
/// 實際頂部 + 上框線寬（2.0pt）+ 上內距（2.75pt）+ posOffset（0.05pt），容差 ±2pt。
/// 同時驗證：（1）跨頁推到次頁時跟隨次頁列頂；（2）同頁前有內容／續列推下時跟隨實際列頂（526.75 ± 2pt）。
#[test]
fn g1_lamp2_vertical_paragraph_origin_dynamic_row_top_and_border_padding() {
    // 子測 A：g1_anchor_paging.docx 跨頁後，錨定列在落點頁的頂部橫線 y 與圖片 y 差值 ＝ 4.75 ± 2.0pt。
    let eng = load_g1_fixture();
    let anchor_page = find_page_with_text(&eng, "尾部段落 0").expect("錨定列落點頁");
    let rules = anchor_table_h_rules(&eng, anchor_page);
    assert!(
        !rules.is_empty(),
        "p{anchor_page} 必須含錨定表 2pt 橫框線，實得空"
    );
    let host_row_top = rules[0];
    let imgs = page_anchor_images(&eng, anchor_page);
    assert_eq!(
        imgs.len(),
        2,
        "p{anchor_page} 必須有 2 枚錨點圖（修前掛在上一頁且 y=525.65）"
    );
    for (idx, (_x, y, _w, _h)) in imgs.iter().enumerate() {
        let expected_y = host_row_top + LO_ROW_OFFSET_Y;
        assert!(
            (*y - expected_y).abs() <= TOL_PT,
            "燈2A（落點A+C）：圖 {idx} y={y} 應為宿主列實際頂 {host_row_top} + {LO_ROW_OFFSET_Y} = {expected_y} (±{TOL_PT}pt)"
        );
    }

    // 子測 B：當錨定表排在頁面首部（同 LO p4：列 0 頂=72pt、高 450pt → 列 1 頂=522.0pt）時，
    // 兩枚圖片 y 必須命中 LO 實抓真值 526.75 ± 2.0pt；且若表格前面有前置區塊把表格推到頁中，
    // 圖片 y 亦隨實際列頂下移，絕不滯留 72.1pt。
    let ns = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
    let make_anchor = |relh: u32, pos_h: &str, rid: &str| -> String {
        format!(
            r#"<w:drawing><wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="{relh}" behindDoc="0" locked="0" layoutInCell="0" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column">{pos_h}</wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>635</wp:posOffset></wp:positionV><wp:extent cx="2567305" cy="1443990"/><wp:wrapSquare wrapText="largest"/><wp:docPr id="{relh}" name="P{relh}"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="{relh}" name="P{relh}"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="2567305" cy="1443990"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing>"#
        )
    };
    let anchor_a = make_anchor(2, "<wp:align>center</wp:align>", "rIdImgA");
    let anchor_b = make_anchor(3, "<wp:posOffset>0</wp:posOffset>", "rIdImgB");
    let doc_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {ns}><w:body><w:tbl><w:tblPr><w:tblW w:w="8306" w:type="dxa"/><w:tblInd w:w="55" w:type="dxa"/><w:tblLayout w:type="fixed"/><w:tblBorders><w:top w:val="single" w:sz="16" w:space="0" w:color="000000"/><w:left w:val="single" w:sz="16" w:space="0" w:color="000000"/><w:bottom w:val="single" w:sz="16" w:space="0" w:color="000000"/><w:right w:val="single" w:sz="16" w:space="0" w:color="000000"/><w:insideH w:val="single" w:sz="16" w:space="0" w:color="000000"/><w:insideV w:val="single" w:sz="16" w:space="0" w:color="000000"/></w:tblBorders><w:tblCellMar><w:top w:w="55" w:type="dxa"/><w:left w:w="55" w:type="dxa"/><w:bottom w:w="55" w:type="dxa"/><w:right w:w="55" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w="4153"/><w:gridCol w:w="4153"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="9000" w:hRule="exact"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="4153" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>首列</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="4153" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>首列右</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:trPr><w:trHeight w:val="2955" w:hRule="atLeast"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="4153" w:type="dxa"/></w:tcPr><w:p><w:r>{anchor_a}</w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="4153" w:type="dxa"/></w:tcPr><w:p><w:r>{anchor_b}</w:r></w:p></w:tc></w:tr></w:tbl><w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:left="1800" w:right="1800" w:gutter="0" w:header="0" w:top="1440" w:footer="0" w:bottom="1440"/></w:sectPr></w:body></w:document>"#
    );
    let eng_same = Engine::from_docx_bytes(&build_minimal_docx(&doc_xml)).unwrap();
    let rules_p0 = anchor_table_h_rules(&eng_same, 0);
    let imgs_p0 = page_anchor_images(&eng_same, 0);
    assert_eq!(imgs_p0.len(), 2, "同頁雙列錨定表應在 p0 發射 2 枚浮動圖");
    // 檢查相對第 1 列頂（rules_p0[1]）的補償量為 4.80pt（LO 4.75pt，修前僅 0.05pt）
    let row1_top = rules_p0[1];
    for (idx, (_x, y, _w, _h)) in imgs_p0.iter().enumerate() {
        let dy = *y - row1_top;
        assert!(
            (dy - LO_ROW_OFFSET_Y).abs() <= TOL_PT,
            "燈2B（落點C）：圖 {idx} 相對列頂 {row1_top} 的偏移 dy={dy} 應為 LO {LO_ROW_OFFSET_Y}±{TOL_PT}pt（修前 0.05pt，缺框線 2.0＋上內距 2.75）"
        );
        assert!(
            (*y - LO_SAME_PAGE_Y).abs() <= 4.0,
            "燈2B：圖 {idx} y={y} 應落於 LO 真值 {LO_SAME_PAGE_Y} 鄰域"
        );
    }

    // 子測 C（用戶症狀直接復現）：前段有內容把錨定表推至頁中（y > 250pt），錨點在錨定表首列（row 0）。
    // 修前靜態假設表格從版心頂起算，圖片滯留 y=72.05pt；修後必須等於實際表頂 + 4.80pt。
    let mut lead_paras = String::new();
    for i in 0..14 {
        lead_paras.push_str(&format!(
            "<w:p><w:r><w:t>前置推下段落 {i}</w:t></w:r></w:p>"
        ));
    }
    let mid_doc_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {ns}><w:body>{lead_paras}<w:tbl><w:tblPr><w:tblW w:w="8306" w:type="dxa"/><w:tblInd w:w="55" w:type="dxa"/><w:tblLayout w:type="fixed"/><w:tblBorders><w:top w:val="single" w:sz="16" w:space="0" w:color="000000"/><w:left w:val="single" w:sz="16" w:space="0" w:color="000000"/><w:bottom w:val="single" w:sz="16" w:space="0" w:color="000000"/><w:right w:val="single" w:sz="16" w:space="0" w:color="000000"/><w:insideH w:val="single" w:sz="16" w:space="0" w:color="000000"/><w:insideV w:val="single" w:sz="16" w:space="0" w:color="000000"/></w:tblBorders><w:tblCellMar><w:top w:w="55" w:type="dxa"/><w:left w:w="55" w:type="dxa"/><w:bottom w:w="55" w:type="dxa"/><w:right w:w="55" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w="4153"/><w:gridCol w:w="4153"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="2955" w:hRule="atLeast"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="4153" w:type="dxa"/></w:tcPr><w:p><w:r>{anchor_a}</w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="4153" w:type="dxa"/></w:tcPr><w:p><w:r>{anchor_b}</w:r></w:p></w:tc></w:tr></w:tbl><w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:left="1800" w:right="1800" w:gutter="0" w:header="0" w:top="1440" w:footer="0" w:bottom="1440"/></w:sectPr></w:body></w:document>"#
    );
    let eng_mid = Engine::from_docx_bytes(&build_minimal_docx(&mid_doc_xml)).unwrap();
    let mid_rules = anchor_table_h_rules(&eng_mid, 0);
    let mid_imgs = page_anchor_images(&eng_mid, 0);
    assert_eq!(mid_imgs.len(), 2, "頁中錨定表應在 p0 發射 2 枚浮動圖");
    let mid_tbl_top = mid_rules[0];
    assert!(
        mid_tbl_top > 250.0,
        "前置段落應將錨定表推至 y > 250pt，實得表頂 {mid_tbl_top}"
    );
    for (idx, (_x, y, _w, _h)) in mid_imgs.iter().enumerate() {
        let expected_y = mid_tbl_top + LO_ROW_OFFSET_Y;
        assert!(
            (*y - expected_y).abs() <= TOL_PT,
            "燈2C（落點A）：頁中表圖 {idx} y={y} 應為實際表頂 {mid_tbl_top} + {LO_ROW_OFFSET_Y} = {expected_y} (±{TOL_PT}pt；修前滯留版心頂 72.05）"
        );
    }
}

/// 燈 3（落點 D）：H 軸 `positionH relativeFrom="column"` 原點取欄內容盒左緣
/// （表左 ＋ 格左 ＋ ½左框線寬 1.0pt ＋ 左內距 2.75pt），且 `align=center` 按內容盒寬置中。
#[test]
fn g1_lamp3_horizontal_column_content_box_and_center_align() {
    let eng = load_g1_fixture();
    // 不論掛在哪一頁，找出兩枚錨點圖驗證水平座標
    let mut imgs: Vec<(f32, f32, f32, f32)> = (0..eng.page_count())
        .flat_map(|p| page_anchor_images(&eng, p))
        .collect();
    assert_eq!(imgs.len(), 2, "全文件應恰有 2 枚錨點圖");
    imgs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    let (ax, _ay, aw, ah) = imgs[0];
    let (bx, _by, bw, bh) = imgs[1];
    let gap = bx - ax;

    assert!(
        (aw - LO_IMG_W).abs() <= 0.5 && (ah - LO_IMG_H).abs() <= 0.5,
        "A 圖尺寸 ({aw}×{ah}) 應為 ({LO_IMG_W}×{LO_IMG_H})"
    );
    assert!(
        (bw - LO_IMG_W).abs() <= 0.5 && (bh - LO_IMG_H).abs() <= 0.5,
        "B 圖尺寸 ({bw}×{bh}) 應為 ({LO_IMG_W}×{LO_IMG_H})"
    );
    assert!(
        (ax - LO_IMG_A_X).abs() <= TOL_PT,
        "燈3（落點D）：A 圖（align=center）x={ax} 應命中 LO 真值 {LO_IMG_A_X}±{TOL_PT}pt（修前 95.50）"
    );
    assert!(
        (bx - LO_IMG_B_X).abs() <= TOL_PT,
        "燈3（落點D）：B 圖（posOffset=0）x={bx} 應命中 LO 真值 {LO_IMG_B_X}±{TOL_PT}pt（修前 300.40，缺 ½框線 1.0＋左內距 2.75）"
    );
    assert!(
        (gap - LO_IMG_AB_GAP).abs() <= TOL_PT,
        "燈3（落點D）：兩圖水平間距 (bx - ax)={gap} 應為 LO 真值 {LO_IMG_AB_GAP}±{TOL_PT}pt（修前 204.90）"
    );
}

/// 燈 4（多欄 `w:cols` ＋ `behindDoc="0"` z 序）：
/// 1. 雙欄節（`w:cols w:num="2" w:space="720"`）次欄段落內的 `positionH relativeFrom="column"`
///    錨點圖，x 原點必須取自次欄實際內容盒起點（72 + 207.65 + 36 = 315.65pt），不得用版心左緣 72pt。
/// 2. `g1_anchor_paging.docx` 中 `behindDoc="0"` 錨點圖在顯示列表中的順序必須位於同頁表格框線之後，
///    且依 `relativeHeight`（A=2 在 B=3 之前）發射。
#[test]
fn g1_lamp4_multicolumn_w_cols_and_z_order() {
    let ns = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
    let col2_anchor = r#"<w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="5" behindDoc="0" locked="0" layoutInCell="0" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>127000</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="1270000" cy="635000"/><wp:wrapSquare wrapText="largest"/><wp:docPr id="5" name="Col2Pic"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="5" name="Col2Pic"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rIdImgA"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1270000" cy="635000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing>"#;
    // A4 11906×16838，margin 1440 (72pt)，2 欄 space=720 (36pt) → 欄寬 207.65pt，次欄起點 315.65pt。
    // 塞 45 段填滿第 0 欄（每頁容量約 41 帶），第 44 段落在次欄（col=1），帶 posOffset=10pt (127000 EMU)。
    let mut paras = String::new();
    for i in 0..44 {
        paras.push_str(&format!(
            "<w:p><w:r><w:t>首欄填充段落 {i}</w:t></w:r></w:p>"
        ));
    }
    paras.push_str(&format!(
        "<w:p><w:r><w:t>次欄錨點宿主段</w:t>{col2_anchor}</w:r></w:p>"
    ));
    let doc_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {ns}><w:body>{paras}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:left="1440" w:right="1440" w:gutter="0" w:header="0" w:top="1440" w:footer="0" w:bottom="1440"/><w:cols w:num="2" w:space="720"/></w:sectPr></w:body></w:document>"#
    );
    let eng_cols = Engine::from_docx_bytes(&build_minimal_docx(&doc_xml)).unwrap();
    let dl0 = eng_cols.page_display_list(0).expect("p0 存在");
    let host_text_x = dl0
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text { x, text, .. } if text.contains("次欄錨點宿主段") => Some(*x),
            _ => None,
        })
        .expect("次欄錨點宿主段應在 p0 次欄");
    assert!(
        (host_text_x - 315.65).abs() <= 1.0,
        "宿主段文字應在次欄起點 315.65pt，實得 {host_text_x}"
    );
    let col2_img_x = dl0
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Image { x, w, h, .. }
                if (*w - 100.0).abs() < 1.0 && (*h - 50.0).abs() < 1.0 =>
            {
                Some(*x)
            }
            _ => None,
        })
        .expect("p0 必須發射次欄 100×50pt 錨點圖");
    let expected_col2_img_x = 315.65 + 10.0; // 次欄起點 315.65 + posOffset 10pt
    assert!(
        (col2_img_x - expected_col2_img_x).abs() <= TOL_PT,
        "燈4（多欄 column 原點）：次欄錨點圖 x={col2_img_x} 應為次欄起點 315.65 + 10.0 = {expected_col2_img_x} (±{TOL_PT}pt；修前偷渡版心左緣得 82.0）"
    );

    // 驗證 g1_anchor_paging.docx 中 behindDoc="0" 錨點圖繪於同頁表格框線（Rule）之後，且依 relativeHeight (2 → 3) 排序。
    let eng = load_g1_fixture();
    let anchor_page = find_page_with_text(&eng, "尾部段落 0").expect("錨定列落點頁");
    let dl = eng.page_display_list(anchor_page).expect("落點頁存在");
    let last_tbl_rule_idx = dl
        .items
        .iter()
        .rposition(
            |it| matches!(it, DrawItem::Rule { width_pt, .. } if (*width_pt - 2.0).abs() < 0.2),
        )
        .expect("落點頁應有錨定表 2pt 框線");
    let img_indices: Vec<(usize, f32)> = dl
        .items
        .iter()
        .enumerate()
        .filter_map(|(idx, it)| match it {
            DrawItem::Image { x, w, h, .. }
                if (*w - LO_IMG_W).abs() < 1.5 && (*h - LO_IMG_H).abs() < 1.5 =>
            {
                Some((idx, *x))
            }
            _ => None,
        })
        .collect();
    assert_eq!(img_indices.len(), 2, "落點頁應發射 2 枚錨點圖");
    assert!(
        img_indices[0].0 > last_tbl_rule_idx && img_indices[1].0 > last_tbl_rule_idx,
        "behindDoc=0 錨點圖必須在表格框線之後發射（浮於表格之上）：last_rule={last_tbl_rule_idx}, imgs={img_indices:?}"
    );
    assert!(
        img_indices[0].1 < img_indices[1].1,
        "relativeHeight=2 (A, 左) 應先於 relativeHeight=3 (B, 右) 發射：{img_indices:?}"
    );
}

/// 燈 5（`G1_BUGGY=1` 回塞鑑別鎖）：當環境變數 `G1_BUGGY=1` 啟用時，退回修前靜態原點與流起頭頁附掛，
/// 驗證燈 1／燈 2／燈 3 的斷言目標確由 G1 修復驅動。
#[test]
fn g1_buggy_env_regression_lock() {
    if std::env::var_os("G1_BUGGY").is_none() {
        eprintln!("未設 G1_BUGGY：略過回塞態斷言（以 G1_BUGGY=1 執行可驗證修前退化態）。");
        return;
    }
    let eng = load_g1_fixture();
    let row0_page = find_page_with_text(&eng, "錨表首列墊高文字").expect("首列頁");
    let imgs_row0 = page_anchor_images(&eng, row0_page);
    assert_eq!(
        imgs_row0.len(),
        2,
        "G1_BUGGY=1 應退回修前行為：圖片錯掛在錨定表起頭頁 p{row0_page}"
    );
    let mut xs: Vec<f32> = imgs_row0.iter().map(|i| i.0).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!(
        (xs[0] - 95.50).abs() < 0.5 && (xs[1] - 300.40).abs() < 0.5,
        "G1_BUGGY=1 應退回修前 x=(95.50, 300.40)，實得 {xs:?}"
    );
}

/// 用戶真實檔案考卷（`QUIRE_G1_FIXTURE`）：未設定環境變數時自動 skip；
/// 設定時只讀驗證錨點照片不滯留版心頂 y=72.1pt，且水平落入雙欄內容盒。
#[test]
fn g1_user_fixture_env_gated() {
    let Ok(path) = std::env::var("QUIRE_G1_FIXTURE") else {
        eprintln!("未設定 QUIRE_G1_FIXTURE：略過外部真實檔案測試。");
        return;
    };
    let path = path.trim();
    if path.is_empty() {
        return;
    }
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("讀取 QUIRE_G1_FIXTURE 失敗: {e}"));
    let eng = Engine::from_docx_bytes(&bytes).expect("解析 QUIRE_G1_FIXTURE 應成功");
    let mut found_pair = None;
    for p in 0..eng.page_count() {
        let imgs = page_anchor_images(&eng, p);
        if imgs.len() >= 2 {
            found_pair = Some((p, imgs));
            break;
        }
    }
    let (page, mut imgs) = found_pair.expect("應在某頁找到兩張並排錨點照片（202.15×113.70pt）");
    imgs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let (ax, ay, _, _) = imgs[0];
    let (bx, by, _, _) = imgs[1];
    assert!(
        ay > 265.0 && by > 265.0,
        "p{page} 並排照片不得滯留版心頂 y=72.1pt 被前段續表壓住：實得 ay={ay}, by={by}"
    );
    assert!(
        (ax - LO_IMG_A_X).abs() <= TOL_PT && (bx - LO_IMG_B_X).abs() <= TOL_PT,
        "p{page} 並排照片 x 應落於 LO 真值 ({LO_IMG_A_X}, {LO_IMG_B_X})±{TOL_PT}pt，實得 ({ax}, {bx})"
    );
}
