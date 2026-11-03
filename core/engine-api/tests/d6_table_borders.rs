//! D6 發射面斷言考卷：WC048 表格網格線段與合併格邊框發射。
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。
//!
//! # 考卷發射面斷言：
//! 1. WC048 表格網格：對 LO PDF 抽線段（pymupdf `get_drawings()` 的豎線 x 集），我方發射的 cell 邊框線 x/y 命中 LO ±2pt，
//!    缺失線段數 = 0（先紅拍出現缺哪些線，逐條列 bbox）。
//! 2. 合併格發射面：vMerge continuation cell 的底邊與側邊在發射面必須發射出 DrawItem::Rule，
//!    末格承接整個合併格底框，側邊連續不缺段，禁按「合併格無框」偷懶導致邊框丟失。

use quire_engine::Engine;
use quire_render_trait::DrawItem;

const WC048: &str = "xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx";

fn load_corpus(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open corpus docx")
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct LineBBox {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
}

/// 燈 1：WC048 表格網格發射面斷言。
///
/// LO PDF 真值（pymupdf `get_drawings()`，線寬 0.5pt，黑色）：
/// - 4 條豎線 x 集：[72.30, 222.50, 372.80, 523.10]，y 覆蓋 [72.00, 358.90]
/// - 4 條橫線 y 集：[72.25, 138.15, 344.75, 358.65]，x 覆蓋 [72.05, 523.35]
///
/// 我方發射的 cell 邊框線 x/y 必須命中 LO ±2pt，缺失線段數 = 0。
#[test]
fn lamp1_wc048_table_grid_emitted_rules_match_lo() {
    let eng = load_corpus(WC048);
    let dl = eng.page_display_list(0).expect("p0 存在");

    // 收集我方發射的所有表格邊框線（width 0.5pt，黑色）
    let mut table_rules: Vec<LineBBox> = Vec::new();
    println!("=== dump 邊框 item (WC048 p0) ===");
    for it in &dl.items {
        if let DrawItem::Rule {
            x1,
            y1,
            x2,
            y2,
            width_pt,
            color_argb,
            ..
        } = it
        {
            if (*width_pt - 0.5).abs() < 0.1 && *color_argb == 0xFF00_0000 {
                let bbox = LineBBox {
                    x1: x1.min(*x2),
                    y1: y1.min(*y2),
                    x2: x1.max(*x2),
                    y2: y1.max(*y2),
                };
                println!(
                    "Rule: ({:.2}, {:.2}) -> ({:.2}, {:.2}), w={:.2}",
                    bbox.x1, bbox.y1, bbox.x2, bbox.y2, width_pt
                );
                table_rules.push(bbox);
            }
        }
    }
    println!("我方表格邊框 Rule 總數: {}", table_rules.len());

    // LO PDF 真值 4 條豎線（pymupdf get_drawings() 實測真值）
    let lo_vert_truth = [
        LineBBox {
            x1: 72.30,
            y1: 72.00,
            x2: 72.30,
            y2: 358.90,
        },
        LineBBox {
            x1: 222.50,
            y1: 72.50,
            x2: 222.50,
            y2: 358.40,
        },
        LineBBox {
            x1: 372.80,
            y1: 72.50,
            x2: 372.80,
            y2: 358.40,
        },
        LineBBox {
            x1: 523.10,
            y1: 72.00,
            x2: 523.10,
            y2: 358.90,
        },
    ];

    // LO PDF 真值 4 條橫線
    let lo_horz_truth = [
        LineBBox {
            x1: 72.05,
            y1: 72.25,
            x2: 523.35,
            y2: 72.25,
        },
        LineBBox {
            x1: 72.55,
            y1: 138.15,
            x2: 522.85,
            y2: 138.15,
        },
        LineBBox {
            x1: 72.55,
            y1: 344.75,
            x2: 522.85,
            y2: 344.75,
        },
        LineBBox {
            x1: 72.05,
            y1: 358.65,
            x2: 523.35,
            y2: 358.65,
        },
    ];

    let mut missing_lines: Vec<String> = Vec::new();

    // 1. 豎線 x 集與覆蓋檢查：
    for (i, lo) in lo_vert_truth.iter().enumerate() {
        let matching: Vec<&LineBBox> = table_rules
            .iter()
            .filter(|r| (r.x1 - r.x2).abs() < 0.5 && (r.x1 - lo.x1).abs() <= 2.0)
            .collect();
        if matching.is_empty() {
            missing_lines.push(format!(
                "LO 豎線 {} 缺失：bbox=({:.2}, {:.2}, {:.2}, {:.2})",
                i, lo.x1, lo.y1, lo.x2, lo.y2
            ));
        } else {
            let y_min = matching.iter().map(|r| r.y1).fold(f32::INFINITY, f32::min);
            let y_max = matching
                .iter()
                .map(|r| r.y2)
                .fold(f32::NEG_INFINITY, f32::max);
            if (y_min - lo.y1).abs() > 2.0 || (y_max - lo.y2).abs() > 2.0 {
                missing_lines.push(format!(
                    "LO 豎線 {} 端點未命中 ±2pt：LO=[{:.2}, {:.2}] vs 我方=[{:.2}, {:.2}]，bbox=({:.2}, {:.2}, {:.2}, {:.2})",
                    i, lo.y1, lo.y2, y_min, y_max, lo.x1, lo.y1, lo.x2, lo.y2
                ));
            }
        }
    }

    // 2. 橫線 y 集與覆蓋檢查：
    for (i, lo) in lo_horz_truth.iter().enumerate() {
        let matching: Vec<&LineBBox> = table_rules
            .iter()
            .filter(|r| (r.y1 - r.y2).abs() < 0.5 && (r.y1 - lo.y1).abs() <= 2.0)
            .collect();
        if matching.is_empty() {
            missing_lines.push(format!(
                "LO 橫線 {} 缺失：bbox=({:.2}, {:.2}, {:.2}, {:.2})",
                i, lo.x1, lo.y1, lo.x2, lo.y2
            ));
        } else {
            let x_min = matching.iter().map(|r| r.x1).fold(f32::INFINITY, f32::min);
            let x_max = matching
                .iter()
                .map(|r| r.x2)
                .fold(f32::NEG_INFINITY, f32::max);
            if (x_min - lo.x1).abs() > 2.0 || (x_max - lo.x2).abs() > 2.0 {
                missing_lines.push(format!(
                    "LO 橫線 {} 端點未命中 ±2pt：LO=[{:.2}, {:.2}] vs 我方=[{:.2}, {:.2}]，bbox=({:.2}, {:.2}, {:.2}, {:.2})",
                    i, lo.x1, lo.x2, x_min, x_max, lo.x1, lo.y1, lo.x2, lo.y2
                ));
            }
        }
    }

    if std::env::var_os("D6_BORDERS_BUGGY").is_some() {
        missing_lines.push("D6_BORDERS_BUGGY=1: 人工回塞拍紅".into());
    }

    if !missing_lines.is_empty() {
        println!("\n【修前紅燈檢測】現缺線段清單（逐條列 bbox）：");
        for (idx, line) in missing_lines.iter().enumerate() {
            println!("  [{}] {}", idx + 1, line);
        }
    }

    assert_eq!(
        missing_lines.len(),
        0,
        "缺失線段數應為 0（先紅拍出現缺哪些線），實缺 {} 條",
        missing_lines.len()
    );
}

/// 燈 2：合併格情境發射面斷言（跨欄與跨列組合）。
///
/// vMerge continuation cell 的底線繼承規則按 ECMA-376 §17.4.4（tcBorders 沿用規則）實作，
/// 末格必須發射出底邊 Rule，側邊必須發射出垂直線段，禁按「合併格無框」偷懶導致邊框線段缺失。
#[test]
fn lamp2_vmerge_continuation_emits_bottom_rule() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:tbl>
      <w:tblGrid>
        <w:gridCol w:w="3000"/>
        <w:gridCol w:w="3000"/>
      </w:tblGrid>
      <w:tr>
        <w:tc>
          <w:tcPr>
            <w:gridSpan w:val="2"/>
            <w:vMerge w:val="restart"/>
            <w:tcBorders>
              <w:bottom w:val="single" w:sz="16" w:space="0" w:color="000000"/>
              <w:left w:val="single" w:sz="8" w:space="0" w:color="000000"/>
              <w:right w:val="single" w:sz="8" w:space="0" w:color="000000"/>
            </w:tcBorders>
          </w:tcPr>
          <w:p><w:r><w:t>Head Span 2</w:t></w:r></w:p>
        </w:tc>
      </w:tr>
      <w:tr>
        <w:tc>
          <w:tcPr>
            <w:gridSpan w:val="2"/>
            <w:vMerge w:val="continue"/>
          </w:tcPr>
          <w:p><w:r><w:t>Tail Span 2</w:t></w:r></w:p>
        </w:tc>
      </w:tr>
    </w:tbl>
  </w:body>
</w:document>"#;

    let mut zip_builder = ZipTestHelper::new();
    zip_builder.add(
        "[Content_Types].xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#,
    );
    zip_builder.add(
        "_rels/.rels",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#,
    );
    zip_builder.add("word/document.xml", xml.as_bytes());
    let docx_bytes = zip_builder.finish();

    let eng = Engine::from_docx_bytes(&docx_bytes).expect("open vmerge docx");
    let dl = eng.page_display_list(0).expect("p0 存在");

    println!("=== dump 合併格發射 Rule items ===");
    let mut rules = Vec::new();
    for it in &dl.items {
        if let DrawItem::Rule {
            x1,
            y1,
            x2,
            y2,
            width_pt,
            ..
        } = it
        {
            println!(
                "Rule: ({:.2}, {:.2}) -> ({:.2}, {:.2}), w={:.2}",
                x1, y1, x2, y2, width_pt
            );
            rules.push((*x1, *y1, *x2, *y2, *width_pt));
        }
    }

    let mut missing = Vec::new();

    // 1. 底邊檢驗：Row 1 結束位置應在 y≈105.6（72.0 + 16.8 + 16.8 = 105.6±1pt）。
    // 在修前，底邊被誤畫在 Row 0 底（y=88.8），而 Row 1 底邊缺失！
    let bottom_rule = rules.iter().find(|(x1, y1, x2, y2, w)| {
        (*w - 2.0).abs() < 0.1
            && (y1 - y2).abs() < 0.5
            && (*y1 - 105.6).abs() <= 2.0
            && (*x1 - 72.0).abs() <= 1.0
            && (*x2 - 372.0).abs() <= 1.0
    });
    if bottom_rule.is_none() {
        missing.push(format!(
            "合併格底邊邊框缺失：期望在 y≈105.6 發射 [72.0, 105.6, 372.0, 105.6] w=2.0，實得 None"
        ));
    }

    // 2. 側邊檢驗：Row 1（continuation cell，y: 88.8 -> 105.6）的左緣與右緣必須發射出邊框 Rule
    let left_rule = rules.iter().find(|(x1, y1, x2, y2, w)| {
        (*w - 1.0).abs() < 0.1
            && (x1 - x2).abs() < 0.5
            && (*x1 - 72.0).abs() <= 1.0
            && y1.min(*y2) <= 89.0
            && y1.max(*y2) >= 104.0
    });
    if left_rule.is_none() {
        missing.push(format!(
            "合併格 continuation cell 左緣垂直分隔線缺段：期望在 x=72.0 覆蓋 y=[88.8, 105.6]，實得缺失"
        ));
    }

    let right_rule = rules.iter().find(|(x1, y1, x2, y2, w)| {
        (*w - 1.0).abs() < 0.1
            && (x1 - x2).abs() < 0.5
            && (*x1 - 372.0).abs() <= 1.0
            && y1.min(*y2) <= 89.0
            && y1.max(*y2) >= 104.0
    });
    if right_rule.is_none() {
        missing.push(format!(
            "合併格 continuation cell 右緣邊框丟失：期望在 x=372.0 覆蓋 y=[88.8, 105.6]，實得缺失"
        ));
    }

    if std::env::var_os("D6_BORDERS_BUGGY").is_some() {
        missing.push("D6_BORDERS_BUGGY=1: 人工回塞拍紅".into());
    }

    if !missing.is_empty() {
        println!("\n【修前紅燈檢測】合併格現缺線段清單（逐條列 bbox）：");
        for (idx, line) in missing.iter().enumerate() {
            println!("  [{}] {}", idx + 1, line);
        }
    }

    assert_eq!(
        missing.len(),
        0,
        "燈 2 失敗：合併格缺失線段數應為 0，實缺 {} 條（修前肉眼實錘缺陷）",
        missing.len()
    );
}

struct ZipTestHelper {
    buf: Vec<u8>,
    centrals: Vec<Vec<u8>>,
    count: usize,
}

impl ZipTestHelper {
    fn new() -> Self {
        Self {
            buf: Vec::new(),
            centrals: Vec::new(),
            count: 0,
        }
    }

    fn add(&mut self, name: &str, data: &[u8]) {
        let offset = self.buf.len() as u64;
        let csize = data.len() as u64;
        let mut crc = flate2::Crc::new();
        crc.update(data);
        let crc = crc.sum();

        self.buf.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        self.buf.extend_from_slice(&20u16.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&0x1234u16.to_le_bytes());
        self.buf.extend_from_slice(&0x5678u16.to_le_bytes());
        self.buf.extend_from_slice(&crc.to_le_bytes());
        self.buf.extend_from_slice(&(csize as u32).to_le_bytes());
        self.buf.extend_from_slice(&(csize as u32).to_le_bytes());
        self.buf
            .extend_from_slice(&(name.len() as u16).to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(name.as_bytes());
        self.buf.extend_from_slice(data);

        let mut c = Vec::new();
        c.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        c.extend_from_slice(&0x031Eu16.to_le_bytes());
        c.extend_from_slice(&20u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0x1234u16.to_le_bytes());
        c.extend_from_slice(&0x5678u16.to_le_bytes());
        c.extend_from_slice(&crc.to_le_bytes());
        c.extend_from_slice(&(csize as u32).to_le_bytes());
        c.extend_from_slice(&(csize as u32).to_le_bytes());
        c.extend_from_slice(&(name.len() as u16).to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&(offset as u32).to_le_bytes());
        c.extend_from_slice(name.as_bytes());
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

        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(self.count as u16).to_le_bytes());
        out.extend_from_slice(&(self.count as u16).to_le_bytes());
        out.extend_from_slice(&(cd_size as u32).to_le_bytes());
        out.extend_from_slice(&(cd_off as u32).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }
}
