//! ODF 文字走查器共用工作模組（卡 t_4006056b）。
//!
//! 動機：`quire-odf` 內部有兩套 `content.xml` 文字走查器——
//! 1. `body.rs::extract_body_from_bytes`（`OdfBundle::extract_body()`，最小內文模型）
//! 2. `layout_glue.rs::walk_paragraphs`（`project()` 排版投影）
//!
//! t_ae9a9f22（合併 `7c2c0a9`）把「XML 格式化空白正規化」與 t_2a983570（合併
//! `1a33f81`）把「跨節點 pending_gap 記帳」都只落在 `layout_glue` 側，`body.rs`
//! 側仍是零正規化 ⇒ 同一檔兩口徑每檔差可達數十至數百碼點（字數統計 vs 渲染
//! 對不上）。本模組把**正規化＋節點間 gap 狀態機＋文字節點發射**收成單一實作，
//! 兩走查器同吃——嚴禁在任何一側複製貼上第二份正規化。
//!
//! 語義管轄：正規化與 gap 的**語義**由 t_ae9a9f22／t_2a983570 定案（見
//! `docs/e1cd-xml-space-policy.md` §1–§2 與 `docs/odt-userdoc-layout-diag.md`
//! 軸 2／軸 4），本模組只做搬移，不做改寫。搬移前後 `walk_paragraphs` 的輸出
//! 必須逐字元相同（考卷 `odf_e2d_header_footer` 燈 6 常量釘住宿主文字流零變動）。
//!
//! 回塞鑑別旗標（兩支各自獨立，嚴禁合併——各自守著自己那一側的正規化臂）：
//! * `ODF_WS_BUGGY`：`layout_glue` 文字節點走零正規化舊路徑（燈 6 鑑別，卡
//!   t_ae9a9f22／t_2a983570 管轄）。
//! * `ODF_BODY_WS_BUGGY`：`body` 側走零正規化＋CData 丟棄舊路徑（口徑對等考卷
//!   燈 4 鑑別，卡 t_4006056b 管轄）。

/// run 樣式佔位型別：`layout_glue` 用它記 `(字級 pt, 粗體)`；`body` 側不帶樣式
/// 帳，傳 `BODY_PLAIN_STYLE` 佔位（`TextStream` 的 `TextSink` 實作忽略此參數）。
pub type Style = (f32, bool);

/// `body` 側不記樣式：`emit_text_node` 共用簽名的佔位值（`TextStream` 忽略它）。
pub const BODY_PLAIN_STYLE: Style = (0.0, false);

/// XML 空白字元判定：只認空格、TAB、CR、LF——刻意不用 `char::is_whitespace`
/// （NBSP U+00A0、類空格 U+2007 等是**正文字元**，語料實測存在，動到即吞字）。
#[inline]
pub fn is_xml_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

/// XML 空白四碼的位元組級判定。四碼全是 ASCII，而 UTF-8 多字節序列的第二碼起
/// 恆 ≥ 0x80 ⇒ 以位元組比對不會誤切碼點，也無需每節點 `Vec<char>`（效能路徑）。
#[inline]
fn is_ws_byte(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

/// 格式化空白（CR／LF／TAB）的位元組級判定——純空格不算。
#[inline]
fn is_fmt_byte(b: u8) -> bool {
    matches!(b, b'\t' | b'\r' | b'\n')
}

/// 把「只含 ASCII 空白」或「由非空白位元組切出」的切片還原成字串。切片邊界恆落
/// 在碼點邊界上（理由同上：空白四碼全 ASCII），故 `from_utf8` 必然成功；不可能
/// 路徑退回空字串，不 panic。
#[inline]
fn ws_slice_str(b: &[u8]) -> &str {
    core::str::from_utf8(b).unwrap_or("")
}

/// `normalize_xml_text_with_edge` 的邊界記帳：節點首／尾是否有**含格式化字元的
/// 空白 run 被剷除**（卡 t_2a983570 返修核心）。
///
/// 跨節點語義：被剷除的邊界格式化空白和「整節點純空白」是同一件事——兩者都代表
/// 源檔在這裡原本有空白、正規化把噪音剝掉了。因此呼叫端（`emit_text_node`）必須
/// 為任一旗標補一次 `arm_gap()`，否則相鄰節點文字直接黏連（`甲`＋尾縮排＋`乙`
/// ⇒ `甲乙`，修前實測）。段首段尾的丟棄規則仍由 `TextStream::flush_gap` 統一把關
/// （段落尚無文字／剛過顯式硬換行 ⇒ 不落地）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EdgeInfo {
    /// 節點**首部**含 CR／LF／TAB 的空白 run 被剷除。
    pub lead_fmt_ws_stripped: bool,
    /// 節點**尾端**含 CR／LF／TAB 的空白 run 被剷除。
    pub trail_fmt_ws_stripped: bool,
}

/// 正規化＋邊界記帳版（呼叫端用這個）。契約同 `normalize_xml_text`，另回傳
/// 首尾格式化空白是否被剷除。
///
/// 整節點全為 XML 空白時的分流（卡 t_2a983570 燈 9）：
/// * 含 CR／LF／TAB ⇒ 回傳空字串＋雙旗標：格式化噪音，呼叫端掛「節點間單空格」。
/// * **純空格** ⇒ 原樣回傳（不折疊、不進 gap）：LO rawdict 實測純空格是正文字元，
///   兩個空格渲染兩個空格字形（`docs/e1cd-xml-space-policy.md` §1）。
pub fn normalize_xml_text_with_edge(node: &str) -> (String, EdgeInfo) {
    let bs = node.as_bytes();
    let n = bs.len();
    let mut edge = EdgeInfo::default();
    // 整節點純空白：先分流（含格式化字元 ⇒ gap 語義；純空格 ⇒ 原樣入文）。
    if bs.iter().all(|b| is_ws_byte(*b)) {
        if bs.iter().any(|b| is_fmt_byte(*b)) {
            edge.lead_fmt_ws_stripped = true;
            edge.trail_fmt_ws_stripped = true;
            return (String::new(), edge);
        }
        return (node.to_owned(), edge);
    }
    let mut out: String = String::with_capacity(n);
    let mut i = 0usize;
    // 首段空白 run：含格式化字元 → 整段剷除並記帳；純空格 → 原樣保留。
    let lead_from = i;
    while i < n && is_ws_byte(bs[i]) {
        i += 1;
    }
    let lead = &bs[lead_from..i];
    if !lead.is_empty() {
        if lead.iter().any(|b| is_fmt_byte(*b)) {
            edge.lead_fmt_ws_stripped = true;
        } else {
            out.push_str(ws_slice_str(lead));
        }
    }
    // 其餘：非空白一次吃一整段，空白 run 依「內部／尾段」分流。
    while i < n {
        if !is_ws_byte(bs[i]) {
            let from = i;
            while i < n && !is_ws_byte(bs[i]) {
                i += 1;
            }
            out.push_str(ws_slice_str(&bs[from..i]));
            continue;
        }
        let from = i;
        while i < n && is_ws_byte(bs[i]) {
            i += 1;
        }
        let run = &bs[from..i];
        let fmt = run.iter().any(|b| is_fmt_byte(*b));
        if i == n {
            // 尾段空白：含格式化字元 → 剷除並記帳；純空格 → 保留。
            if fmt {
                edge.trail_fmt_ws_stripped = true;
            } else {
                out.push_str(ws_slice_str(run));
            }
        } else if fmt {
            // 節點**內部**含格式化字元的空白 run：折恰一個空格（不是硬換行）。
            // 這是「折疊」不是「剷除」⇒ 不記 gap 帳，否則會疊出兩個空格。
            out.push(' ');
        } else {
            out.push_str(ws_slice_str(run));
        }
    }
    (out, edge)
}

/// 剝除單一 XML 文字節點首尾的格式化換行／製表符，並把節點**內部**含
/// CR／LF／TAB 的連續空白折成單個半形空格。純函數（字串進出），考卷直接打它。
///
/// 口徑（比照 DOCX 線 `docs/e1cd-xml-space-policy.md` §2 的節點級裁剪）：
/// 1. 節點**首尾**的空白 run：含 CR／LF／TAB → 整段剷除；純空格 → 原樣保留
///    （段首段尾的有意義空格不誤砍）。
/// 2. 節點**內部**空白 run：含 CR／LF／TAB → 折成一個半形空格（**嚴禁把 `\\n`
///    當成硬換行傳進 layout**）；純空格 run → 原樣保留（不折疊、不黏死）。
/// 3. **整節點純空白** → 回傳空字串；「節點間原有單空格」的語義由呼叫端
///    （`TextStream::arm_gap`）以「待發單空格」承載——兩個 `text:span` 之間的
///    格式化空白＝一個空格，不是零、也不是換行。
pub fn normalize_xml_text(node: &str) -> String {
    normalize_xml_text_with_edge(node).0
}

/// 回塞態旗標（layout 側）：`ODF_WS_BUGGY=1` ⇒ 文字節點走「零正規化」舊路徑
/// （直接 `push_str(&unescaped)`，不剝格式化空白、也不掛節點間待發空格），供考卷
/// `odf_xml_whitespace_policy` 燈 6 證「正規化臂真的在被檢查」。正常態永不觸發
/// （比照 `ODF_D1_BUGGY`／`ODF_E2D_BUGGY` 的既有鑑別力紀律）。
pub fn buggy_no_whitespace_normalize() -> bool {
    std::env::var_os("ODF_WS_BUGGY").is_some()
}

/// 回塞態旗標（body 側）：`ODF_BODY_WS_BUGGY=1` ⇒ `body.rs` 回到「零正規化＋
/// CData 丟棄」舊路徑，供口徑對等考卷 `odf_walker_parity` 燈 4 證「body 側的
/// 對齊臂真的在被檢查」。與 `ODF_WS_BUGGY` **刻意獨立**：兩者各自只扳自己那一側，
/// 合併成一支旗標會讓「兩側同時回塞」看起來像正常態。正常態永不觸發。
pub fn buggy_body_no_normalize() -> bool {
    std::env::var_os("ODF_BODY_WS_BUGGY").is_some()
}

/// 段落級「文字流＋待發空格」狀態機——兩走查器共用的單一實作。
///
/// `layout_glue::ParaCtx` 把它嵌在裡面（另外疊 run 樣式帳），`body.rs` 直接持用
/// （不記樣式）。gap 語義只此一處：
/// * `arm_gap()`：整節點純空白、或文字節點首／尾格式化空白被剷除 ⇒ 掛待發單空格。
/// * `flush_gap()`：下一個有內容的文字節點推入前刷成一個空格。段首（`text` 尚空）
///   與段尾（無人來刷）自然丟棄，故 pretty-print 縮排不會留幽靈空格；相鄰多個
///   純空白節點仍是同一個空格（旗標冪等）。
/// * `push_explicit()`：顯式空白元素（`text:s`／`text:tab`／`text:line-break`）是
///   權威空白來源，其周圍格式化縮排屬噪音 ⇒ 待發空格直接放棄，不疊加。
#[derive(Debug, Clone, Default)]
pub struct TextStream {
    text: String,
    /// `text` 的**碼點數**快取：`push_raw` 每推一片都要知道起訖碼點位置，逐次
    /// `chars().count()` 全掃會讓單段成本變成 O(L²)（長段落＋大量節點切片實測
    /// 熱點，卡 t_2a983570 順車修）。此欄位與 `text` 同步維護。
    char_len: usize,
    /// 「待發單空格」旗標，見型別文件。
    pending_gap: bool,
}

impl TextStream {
    pub fn new() -> Self {
        Self::default()
    }

    /// 目前累積的段落文字。
    pub fn text(&self) -> &str {
        &self.text
    }

    /// 收尾取走文字流（段落閉合時呼叫）。
    pub fn into_text(self) -> String {
        self.text
    }

    /// 追加原文片段（不做正規化；呼叫端負責），回傳該片段的**碼點起訖**
    /// `(from, to)`——`ParaCtx` 用它開 run 窗，`body` 側忽略回傳值。
    pub fn push_raw(&mut self, s: &str) -> (usize, usize) {
        let from = self.char_len;
        self.text.push_str(s);
        self.char_len += s.chars().count();
        (from, self.char_len)
    }

    /// 掛「節點間單空格」待發語義（不直接進文字流）。
    pub fn arm_gap(&mut self) {
        self.pending_gap = true;
    }

    /// 放棄待發空格。
    pub fn drop_gap(&mut self) {
        self.pending_gap = false;
    }

    /// 刷待發單空格：段落已有文字、且行內位置不是剛經過顯式硬換行／製表符
    /// 才落地。段首無鄰側＝丟棄；硬換行後的縮排屬該行行首噪音＝同樣丟棄
    /// （LO 在 `<text:line-break/>` 後從行首起排，不會多一個空格）。
    /// 落地時回傳該空格的碼點起訖（供 run 開窗）；未落地回 `None`。
    pub fn flush_gap(&mut self) -> Option<(usize, usize)> {
        if !self.pending_gap {
            return None;
        }
        self.pending_gap = false;
        let at_line_start = self
            .text
            .chars()
            .next_back()
            .is_none_or(|c| matches!(c, '\n' | '\t'));
        if self.text.is_empty() || at_line_start {
            None
        } else {
            Some(self.push_raw(" "))
        }
    }

    /// 正規化後的**內容**節點：先刷待發空格，再推入（呼叫端保證 `s` 非純空白）。
    pub fn push_text_node(&mut self, s: &str) {
        self.flush_gap();
        self.push_raw(s);
    }

    /// 顯式空白元素（`text:s`／`text:tab`／`text:line-break`）展開值入文。
    pub fn push_explicit(&mut self, s: &str) {
        self.drop_gap();
        self.push_raw(s);
    }
}

/// 文字節點發射器的共用介面：`TextStream`（body 側，忽略樣式）與
/// `layout_glue::ParaCtx`（疊 run 樣式帳）各自實作，讓 `emit_text_node` 只有一份。
pub trait TextSink {
    /// 掛節點間待發單空格。
    fn sink_arm_gap(&mut self);
    /// 正規化後非空的內容節點入文（實作自行決定是否先刷待發空格）。
    fn sink_push_content(&mut self, s: &str, style: Style);
}

impl TextSink for TextStream {
    #[inline]
    fn sink_arm_gap(&mut self) {
        TextStream::arm_gap(self);
    }

    #[inline]
    fn sink_push_content(&mut self, s: &str, _style: Style) {
        // body 側不記樣式帳：樣式參數刻意忽略（見 `BODY_PLAIN_STYLE`）。
        TextStream::push_text_node(self, s);
    }
}

/// **單一**文字節點發射路徑（兩走查器的 `Event::Text`／`Event::CData` 都走這裡）。
///
/// 記帳順序是契約的一部分（卡 t_2a983570 燈 7／燈 8）：
/// * 首端記帳要在推入**之前**掛（`flush_gap` 於推入前落地）；
/// * 正規化後為空＝整節點純格式化空白 → 只掛「節點間單空格」；
/// * 尾端記帳在推入**之後**掛（留給下一節點刷）。
///
/// `Event::CData` 亦走本函數（卡 t_4006056b）：CDATA 段在 XML 裡本就是「原文」，
/// quick-xml 不對其內容解析實體，故呼叫端只 `decode()`（字元集＋EOL 正規化）、
/// **不得**再 `unescape`——`<![CDATA[甲 <b> & 乙]]>` 必須原樣成為段落文字。
pub fn emit_text_node<S: TextSink + ?Sized>(sink: &mut S, raw: &str, style: Style) {
    let (normalized, edge) = normalize_xml_text_with_edge(raw);
    if edge.lead_fmt_ws_stripped {
        sink.sink_arm_gap();
    }
    if normalized.is_empty() {
        sink.sink_arm_gap();
    } else {
        sink.sink_push_content(&normalized, style);
    }
    if edge.trail_fmt_ws_stripped {
        sink.sink_arm_gap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 狀態機單元：相鄰多個純空白節點＝同一個空格（旗標冪等），且段首段尾丟棄。
    #[test]
    fn gap_flag_is_idempotent_and_dropped_at_both_ends() {
        let mut s = TextStream::new();
        s.arm_gap();
        s.arm_gap();
        s.push_text_node("甲");
        s.arm_gap();
        s.push_text_node("乙");
        s.arm_gap(); // 段尾無人來刷 ⇒ 丟棄
        assert_eq!(s.text(), "甲 乙");
    }

    /// 狀態機單元：硬換行後的縮排不落地（LO 從行首起排，不多空格）。
    #[test]
    fn gap_after_explicit_line_break_is_dropped() {
        let mut s = TextStream::new();
        s.push_text_node("甲");
        s.push_explicit("\n");
        s.arm_gap();
        s.push_text_node("乙");
        assert_eq!(s.text(), "甲\n乙");
    }

    /// 狀態機單元：顯式空白元素是權威來源 ⇒ 它**之前**掛的待發空格被放棄
    /// （不疊成兩格）；它之後重新掛的待發空格照樣在下一節點前刷出。
    #[test]
    fn explicit_space_drops_the_pending_gap_before_it() {
        let mut s = TextStream::new();
        s.push_text_node("甲");
        s.arm_gap(); // 顯式元素之前的格式化縮排 ⇒ 應被放棄
        s.push_explicit(" ");
        assert_eq!(s.text(), "甲 ", "顯式空格前不得疊出第二格");
        s.arm_gap(); // 顯式元素之後重新掛 ⇒ 照常刷出
        s.push_text_node("乙");
        assert_eq!(
            s.text(),
            "甲  乙",
            "顯式空格之後的新 gap 仍刷一格（正字元為證）"
        );
    }

    /// `push_raw` 的碼點起訖必須以碼點計（非位元組），run 開窗口徑才不會位移。
    #[test]
    fn push_raw_returns_code_point_ranges() {
        let mut s = TextStream::new();
        assert_eq!(s.push_raw("甲乙"), (0, 2));
        assert_eq!(s.push_raw("丙"), (2, 3));
        assert_eq!(s.flush_gap(), None);
        s.arm_gap();
        assert_eq!(s.flush_gap(), Some((3, 4)));
        assert_eq!(s.text(), "甲乙丙 ");
    }

    /// CData 共用路徑：含 `<`／`&` 的原文不被實體化、也不被空白政策誤傷。
    #[test]
    fn cdata_raw_goes_through_the_same_normalization() {
        let mut s = TextStream::new();
        emit_text_node(&mut s, "甲 <b> & 乙", BODY_PLAIN_STYLE);
        assert_eq!(s.text(), "甲 <b> & 乙");
    }
}
