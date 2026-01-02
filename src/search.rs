use std::{cmp::min, sync::atomic::AtomicU64, time::Instant};

use crate::{eval::{eval, eval_piece}, time};
use shakmaty::{Chess, Move, Position, zobrist::{Zobrist64, ZobristHash}};

pub struct NodeCount {
    pub nodes: u64,
    pub leaves: u64,
    pub qnodes: u64,
}

fn move_compare(pos: &Chess, tte: Option<TTEntry>, a: &Move, b: &Move) -> std::cmp::Ordering {
    // TT-move first 
    if let Some(tte) = tte {
        let a_tt: bool = (a.from().unwrap() as u8 == tte.from) && (a.to() as u8 == tte.to);
        let b_tt = (b.from().unwrap() as u8 == tte.from) && (b.to() as u8 == tte.to);
        match (a_tt, b_tt) {
            // Should only be one true, except for promotions to different pieces; order Q>K>R>B
            (true, true) => {
                let a_prom = match a.promotion() {
                    Some(shakmaty::Role::Queen) => 4,
                    Some(shakmaty::Role::Rook) => 3,
                    Some(shakmaty::Role::Bishop) => 2,
                    Some(shakmaty::Role::Knight) => 1,
                    _ => 0,
                };
                let b_prom = match b.promotion() {
                    Some(shakmaty::Role::Queen) => 4,
                    Some(shakmaty::Role::Rook) => 3,
                    Some(shakmaty::Role::Bishop) => 2,
                    Some(shakmaty::Role::Knight) => 1,
                    _ => 0,
                };
                return b_prom.cmp(&a_prom);
            }
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }
    }

    let a_capture = a.capture();
    let b_capture = b.capture();
    match (a_capture, b_capture) {
        (Some(ac), Some(bc)) => {
            // for captures, order by MVV-LVA
            let a_victim_value = eval_piece(a.to(), pos.turn().other(), ac);
            let a_aggres_value = eval_piece(a.from().unwrap(), pos.turn(), pos.board().role_at(a.from().unwrap()).unwrap());

            let b_victim_value = eval_piece(b.to(), pos.turn().other(), bc);
            let b_aggres_value = eval_piece(b.from().unwrap(), pos.turn(), pos.board().role_at(b.from().unwrap()).unwrap());

            b_victim_value.cmp(&a_victim_value).then(b_aggres_value.cmp(&a_aggres_value))
        }
        (Some(_), None) => std::cmp::Ordering::Less, // captures first
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => {
            // for quiet moves, order by piece development (looking at PSQTs)
            let a_role = pos.board().role_at(a.from().unwrap()).unwrap();
            let a_role_to = a.promotion().unwrap_or(a_role);
            let a_value_old = eval_piece(a.from().unwrap(), pos.turn(), a_role);
            let a_value_new = eval_piece(a.to(), pos.turn(), a_role_to);
            let a_devel = a_value_new - a_value_old;

            let b_role = pos.board().role_at(b.from().unwrap()).unwrap();
            let b_role_to = b.promotion().unwrap_or(b_role);
            let b_value_old = eval_piece(b.from().unwrap(), pos.turn(), b_role);
            let b_value_new = eval_piece(b.to(), pos.turn(), b_role_to);
            let b_devel = b_value_new - b_value_old;

            b_devel.cmp(&a_devel)
        },
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
    mut history: Vec<shakmaty::Chess>,
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
            alphabeta(position.clone(), history.clone(), depth_internal, alpha, beta, count, tt);
            get_tt(tt, zob.0)
        } else {
            None
        }
    });

    // three-fold repetition draw detection
    let reps = history.iter().filter(|h| **h == position).count();
    if reps >= 2 {
        // draw
        return (0, Vec::new());
    }
    // fifty-move rule draw detection
    if position.halfmoves() >= 100 {
        return (0, Vec::new());
    }
    history.push(position.clone());


    let mut moves = position.legal_moves();
    if moves.is_empty() {
        if position.is_check() {
            return (-32700, Vec::new());
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
        let hist = if mv.is_zeroing() { Vec::new() } else { history.clone() };
        let (score, sub_pv) = alphabeta(pos, hist, depth - 1, -beta, -alpha, count, tt);
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
    if best_value < -32500  {
        best_value += 1;
    }
    if best_value > 32500  {
        best_value -= 1;
    }
    return (best_value, pv);
}

fn convert_score(score: i16) -> ruci::Score {
    if score > 32000 {
        ruci::Score::MateIn((32701 - score as isize) / 2)
    } else if score < -32000 {
        ruci::Score::MateIn(-(32700 + score) as isize / 2)
    } else {
        ruci::Score::Centipawns(score as isize)
    }
}

pub fn search(
    position: shakmaty::Chess,
    history: Vec<shakmaty::Chess>,
    deadline: time::Deadline,
    tt: &Vec<AtomicU64>,
    callback: &mut dyn FnMut(isize, ruci::Score, &Vec<Move>, &NodeCount),
) -> (ruci::Score, Vec<Move>, NodeCount) {
    let mut count = NodeCount {
        nodes: 0,
        leaves: 0,
        qnodes: 0,
    };
    let mut score = 0;
    let mut pv = Vec::new();
    for d in 0.. {
        (score, pv) = alphabeta(position.clone(), history.clone(), d, i16::MIN + 1, i16::MAX - 1, &mut count, tt);
        callback(d, convert_score(score), &pv, &count);
        if deadline.check_soft(Instant::now(), (count.nodes + count.qnodes - count.leaves) as usize, d as usize) {
            break;
        }
    }
    (convert_score(score), pv, count)
}