use std::sync::atomic::AtomicU64;

use shakmaty::Move;

// Transposition table
// TT-Entry bitmap:
// 24 bits: high part of zobrist key (useful up to 2^40 entries, after that some bits become redundant with index; maybe use buckets at some point?)
// 8 bits: search depth
// 16 bits: score
// 6 bits: from square
// 6 bits: to square
// 2 bits: score type
// 2 bits: free!


#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScoreType {
    Exact = 0,
    LowerBound = 1,
    UpperBound = 2,
}

pub fn move_match_tt(m: &Move, tte: &TTEntry) -> bool {
    (m.from().unwrap() as u8 == tte.from) && (m.to() as u8 == tte.to)
}


#[derive(Clone, Copy)]
pub struct TTEntry {
    // basic best-move info
    pub from: u8,
    pub to: u8,

    // TODO: value info etc
    pub value: i16,
    pub depth: u8,
    pub score_type: ScoreType,
}

pub struct TT(Vec<AtomicU64>);

impl TT {
    pub fn new(size: usize) -> Self {
        let mut v = Vec::new();
        v.resize_with(size, || AtomicU64::new(0));
        TT(v)
    }

    pub fn get(&self, moves: &[Move], key: u64) -> Option<TTEntry> {
        let index = (key % self.0.len() as u64) as usize;
        let entry = self.0[index].load(std::sync::atomic::Ordering::Relaxed);
        if entry >> 40 == (key >> 40) {
            let depth = ((entry >> 32) & 0xFF) as u8;
            let value = ((entry >> 16) & 0xFFFF) as i16;
            let from = ((entry >> 10) & 0x3F) as u8;
            let to = ((entry >> 4) & 0x3F) as u8;
            let score_type = match (entry >> 2) & 0x3 {
                0 => ScoreType::Exact,
                1 => ScoreType::LowerBound,
                2 => ScoreType::UpperBound,
                _ => unreachable!(),
            };
            let entry = TTEntry { from, to, value, depth, score_type };
            if moves.iter().any(|m| move_match_tt(m, &entry)) { Some(entry) } else { None }
        } else {
            None
        }
    }

    pub fn write(&self, key: u64, data: TTEntry) {
        let index = (key % self.0.len() as u64) as usize;
        let entry = (key & 0xFFFFFF0000000000) // basically << 40 but keeping the high part
            | ((data.depth as u64) << 32)
            | ((data.value.cast_unsigned() as u64) << 16)
            | ((data.from as u64) << 10)
            | ((data.to as u64) << 4)
            | ((data.score_type as u64) << 2);
        self.0[index].store(entry, std::sync::atomic::Ordering::Relaxed);
    }

}