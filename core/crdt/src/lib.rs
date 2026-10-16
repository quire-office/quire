//! quire-crdt —— 協作層佔位（Phase 4 落地）。
//!
//! 設計前提：編輯操作的唯一事實來源是 ooxml-ir；CRDT 負責「操作序列的併發合併」，
//! 版面層在最終一致後才重排（不做像素級共感，見研究報告 §9 風險 3）。
//! 後端候選：loro（月下載 50 萬級）或 yrs；適配時只准碰本 crate 的 trait。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EditOp {
    ReplaceRunText {
        para: usize,
        run: usize,
        text: String,
    },
}

pub trait CrdtBackend {
    /// 本地操作提交 → 回傳要廣播給協作者的位元組。
    fn commit_local(&mut self, op: &EditOp) -> Vec<u8>;
    /// 收到遠端位元組 → 回傳需要落地到 ooxml-ir 的操作。
    fn apply_remote(&mut self, bytes: &[u8]) -> Vec<EditOp>;
}

/// 佔位後端（loro/yrs 落地前的可執行佔位，Phase 4 換實作、trait 不變）。
///
/// 合併語意：以 (lamport, session) 全序做 per-(para,run) 的 last-writer-wins。
/// 這是**佔位**用（能跑通管線、可決定性測試），不是最終協作語意——
/// 真正的光標共感/文字級合併屬 loro 適配工作，屆時只准替換本 struct。
#[derive(Debug, Clone)]
pub struct InMemoryBackend {
    session: u64,
    clock: u64,
    /// 每個 (para,run) 目前勝者的 (lamport, session, text)。
    winners: std::collections::BTreeMap<(usize, usize), (u64, u64, String)>,
}

/// 廣播格式：單條加戳操作（serde JSON；決定性鍵序由 BTreeMap/序列化順序保證）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Stamped {
    lamport: u64,
    session: u64,
    op: EditOp,
}

impl InMemoryBackend {
    pub fn new(session: u64) -> Self {
        Self {
            session,
            clock: 0,
            winners: std::collections::BTreeMap::new(),
        }
    }

    /// (para, run) 的當前勝者文字（落地到 ooxml-ir 時查詢用）。
    pub fn current_text(&self, para: usize, run: usize) -> Option<&str> {
        self.winners.get(&(para, run)).map(|(_, _, t)| t.as_str())
    }

    fn beats(&self, lamport: u64, session: u64, key: (usize, usize)) -> bool {
        match self.winners.get(&key) {
            None => true,
            Some((l, s, _)) => (lamport, session) > (*l, *s),
        }
    }
}

impl CrdtBackend for InMemoryBackend {
    fn commit_local(&mut self, op: &EditOp) -> Vec<u8> {
        self.clock += 1;
        let stamped = Stamped {
            lamport: self.clock,
            session: self.session,
            op: op.clone(),
        };
        // EditOp 目前單變體；match 寫法為未來變體（插入/刪除）預留，
        // 非 ReplaceRunText 類操作只廣播、不進 per-run 勝者表。
        match op {
            EditOp::ReplaceRunText { para, run, text } => {
                self.winners
                    .insert((*para, *run), (self.clock, self.session, text.clone()));
            }
        }
        serde_json::to_vec(&stamped).unwrap_or_default()
    }

    fn apply_remote(&mut self, bytes: &[u8]) -> Vec<EditOp> {
        let Ok(stamped) = serde_json::from_slice::<Stamped>(bytes) else {
            return Vec::new();
        };
        // 遠端戳推進本地時鐘（Lamport 規則）。
        self.clock = self.clock.max(stamped.lamport);
        match &stamped.op {
            EditOp::ReplaceRunText { para, run, text } => {
                if !self.beats(stamped.lamport, stamped.session, (*para, *run)) {
                    return Vec::new(); // 敗給現任勝者：不落地，僅合併
                }
                self.winners.insert(
                    (*para, *run),
                    (stamped.lamport, stamped.session, text.clone()),
                );
                vec![stamped.op]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(para: usize, run: usize, text: &str) -> EditOp {
        EditOp::ReplaceRunText {
            para,
            run,
            text: text.to_string(),
        }
    }

    #[test]
    fn concurrent_edits_converge_to_same_winner() {
        // 兩個 session 對同一 (para,run) 併發編輯：兩端合併後必須收斂到同一勝者。
        let mut a = InMemoryBackend::new(1);
        let mut b = InMemoryBackend::new(2);
        let msg_a = a.commit_local(&op(0, 0, "A端"));
        let msg_b = b.commit_local(&op(0, 0, "B端"));
        // 交叉投遞（順序相反）：A 先收 B、B 先收 A。
        assert!(
            !a.apply_remote(&msg_b).is_empty(),
            "(1,2) 勝 (1,1)：A 端應落地 B 操作"
        );
        let _ = b.apply_remote(&msg_a);
        // (lamport=1, session=2) > (lamport=1, session=1)：雙方勝者皆為 session 2。
        assert_eq!(a.current_text(0, 0), Some("B端"));
        assert_eq!(b.current_text(0, 0), Some("B端"));
    }

    #[test]
    fn late_lower_stamp_does_not_regress() {
        let mut a = InMemoryBackend::new(1);
        a.commit_local(&op(3, 1, "較新本地")); // lamport=1, session=1
        let stale = {
            let mut c = InMemoryBackend::new(0);
            c.commit_local(&op(3, 1, "較舊遠端")) // lamport=1, session=0 < (1,1)
        };
        assert!(a.apply_remote(&stale).is_empty(), "敗者操作不得落地");
        assert_eq!(a.current_text(3, 1), Some("較新本地"));
    }

    #[test]
    fn remote_higher_stamp_lands_and_advances_clock() {
        let mut a = InMemoryBackend::new(1);
        let hi = {
            let mut b = InMemoryBackend::new(9);
            b.commit_local(&op(0, 0, "x")); // lamport 1（被 y 覆蓋，不再投遞）
            b.commit_local(&op(0, 0, "y")) // lamport 2 ← 投遞這條
        };
        assert_eq!(a.apply_remote(&hi), vec![op(0, 0, "y")]);
        // 之後本地提交必須排在遠端 lamport 之後（時鐘已推進到 2 → 本地得 3）。
        let mine = a.commit_local(&op(0, 0, "z"));
        let stamped: serde_json::Value = serde_json::from_slice(&mine).unwrap();
        assert_eq!(stamped["lamport"].as_u64(), Some(3));
    }

    #[test]
    fn malformed_bytes_are_ignored() {
        let mut a = InMemoryBackend::new(1);
        assert!(a.apply_remote(b"not json").is_empty());
    }
}
