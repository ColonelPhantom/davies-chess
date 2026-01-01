use std::{cmp::min, sync::atomic::AtomicU64};

use crate::eval::{eval, eval_piece};
use shakmaty::{Chess, Move, Position, zobrist::{Zobrist64, ZobristHash}};

pub struct NodeCount {
    pub nodes: u64,
    pub leaves: u64,
    pub qnodes: u64,
}

fn move_compare(pos: &Chess, tte: Option<TTEntry>, a: &Move, b: &Move) -> std::cmp::Ordering {
    if let Some(tte) = tte {
        let a_tt: bool = (a.from().unwrap() as u8 == tte.from) && (a.to() as u8 == tte.to);
        let b_tt = (b.from().unwrap() as u8 == tte.from) && (b.to() as u8 == tte.to);
        match (a_tt, b_tt) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }
    }

    let a_capture = a.capture();
    let b_capture = b.capture();
    match (a_capture, b_capture) {
        (Some(ac), Some(bc)) => {
            // let a_value = piece_value(ac);
            let a_victim_value = eval_piece(a.to(), pos.turn().other(), ac);
            let a_aggres_value = eval_piece(a.from().unwrap(), pos.turn(), pos.board().role_at(a.from().unwrap()).unwrap());

            let b_victim_value = eval_piece(b.to(), pos.turn().other(), bc);
            let b_aggres_value = eval_piece(b.from().unwrap(), pos.turn(), pos.board().role_at(b.from().unwrap()).unwrap());

            b_victim_value.cmp(&a_victim_value).then(b_aggres_value.cmp(&a_aggres_value))
        }
        (Some(_), None) => std::cmp::Ordering::Less, // captures first
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

#[derive(Clone, Copy)]
struct TTEntry {
    // basic best-move info
    from: u8,
    to: u8,

    // TODO: value info etc
}

fn get_tt(tt: &Vec<AtomicU64>, key: u64) -> Option<TTEntry> {
    let index = (key % tt.len() as u64) as usize;
    let entry = tt[index].load(std::sync::atomic::Ordering::Relaxed);
    if entry >> 32 == (key >> 32) {
        Some(TTEntry {
            from: ((entry >> 24) & 0xFF) as u8,
            to: ((entry >> 16) & 0xFF) as u8,
        })
    } else {
        None
    }
}

fn write_tt(tt: &Vec<AtomicU64>, key: u64, data: TTEntry) {
    let index = (key % tt.len() as u64) as usize;
    let entry = (key & 0xFFFFFFFF00000000) | ((data.from as u64) << 24) | ((data.to as u64) << 16);
    tt[index].store(entry, std::sync::atomic::Ordering::Relaxed);
}

fn qsearch(
    position: shakmaty::Chess,
    mut alpha: i16,
    beta: i16,
    count: &mut NodeCount,
    tt: &Vec<AtomicU64>,
) -> i16 {
    count.qnodes += 1;
    let eval = eval(&position);
    let mut best = eval;
    if best >= beta {
        return best;
    }
    if best > alpha {
        alpha = best;
    }
    let mut moves = position.capture_moves();
    moves.sort_unstable_by(|a,b| move_compare(&position, None, a, b));

    // TODO: move ordering, use SEE-pruning
    for mv in moves {
        let mut pos = position.clone();
        pos.play_unchecked(&mv);
        let score = -qsearch(pos, -beta, -alpha, count, tt);
        if score >= beta {
            return score;
        }
        if score > best {
            best = score;
        }
        if score > alpha {
            alpha = score;
        }
    }
    return best;
}

fn alphabeta(
    position: shakmaty::Chess,
    depth: isize,
    mut alpha: i16,
    beta: i16,
    count: &mut NodeCount,
    tt: &Vec<AtomicU64>,
) -> (i16, Vec<Move>) {
    count.nodes += 1;
    if depth <= 0 {
        // TODO: add qsearch
        count.leaves += 1;
        return (qsearch(position, alpha, beta, count, tt), Vec::new());
    }

    let zob: Zobrist64 = position.zobrist_hash(shakmaty::EnPassantMode::Legal);
    let tt_entry = get_tt(tt, zob.0).or_else(|| {
        // internal iterative deepening
        if depth >= 3 {
            let depth_internal = min(depth - 2, 2);
            alphabeta(position.clone(), depth_internal, alpha, beta, count, tt);
            get_tt(tt, zob.0)
        } else {
            None
        }
    });

    let mut moves = position.legal_moves();
    if moves.is_empty() {
        if position.is_check() {
            return (-10000 + (10 - depth) as i16, Vec::new());
        } else {
            return (0, Vec::new());
        }
    }

    moves.sort_unstable_by(|a,b| move_compare(&position, tt_entry, a, b));

    let mut pv = Vec::new();
    let mut best_value = i16::MIN;
    let mut best_move = moves[0].clone();
    for mv in moves {
        let mut pos = position.clone();
        pos.play_unchecked(&mv);
        let (score, sub_pv) = alphabeta(pos, depth - 1, -beta, -alpha, count, tt);
        let score = -score;
        if score > best_value {
            best_value = score;
            best_move = mv.clone();
            if score > alpha {
                alpha = score;
                pv = sub_pv;
                pv.push(mv);
            }
        }
        if score >= beta {
            // fail-soft
            break;
        }
    }

    write_tt(tt, zob.0, TTEntry {
        from: best_move.from().unwrap() as u8,
        to: best_move.to() as u8,
    });
    return (best_value, pv);
}

pub fn search(
    position: shakmaty::Chess,
    depth: isize,
    tt: &Vec<AtomicU64>,
    callback: &mut dyn FnMut(isize, i16, &Vec<Move>, &NodeCount),
) -> (i16, Vec<Move>, NodeCount) {
    let mut count = NodeCount {
        nodes: 0,
        leaves: 0,
        qnodes: 0,
    };
    let mut score = 0;
    let mut pv = Vec::new();
    if depth > 0 {
        for d in 1..=depth {
            (score, pv) = alphabeta(position.clone(), d, i16::MIN + 1, i16::MAX - 1, &mut count, tt);
            callback(d, score, &pv, &count);
        }
    } else {
        (score, pv) = alphabeta(position.clone(), depth, i16::MIN + 1, i16::MAX - 1, &mut count, tt);
        callback(depth, score, &pv, &count);
    }
    (score, pv, count)
}