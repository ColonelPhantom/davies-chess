use std::{
    cmp::min,
    sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering::Relaxed},
    time::Instant,
};

use crate::{
    eval::{eval, eval_piece},
    time,
    util::sort::LazySort,
};
use shakmaty::{
    Chess, Move, Position, Square, zobrist::{Zobrist64, ZobristHash}
};

pub mod tt;

use tt::*;

pub struct NodeCount {
    pub nodes: AtomicU64,
    pub leaves: AtomicU64,
    pub qnodes: AtomicU64,
    pub seldepth: AtomicIsize,
}

impl NodeCount {
    pub fn count(&self) -> u64 {
        self.nodes.load(Relaxed) + self.qnodes.load(Relaxed) - self.leaves.load(Relaxed)
    }
    pub fn seldepth(&self) -> isize {
        self.seldepth.load(Relaxed)
    }
}

// Move ordering
// note: somewhat confusing, but for the inner values, lower is better
// this is related to how sorting works (lower values earlier)
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum MoveOrderKey {
    TTMove(i16),
    Capture(i16, i16), // victim value, aggressor value
    Quiet(i32),        // development value
}

fn move_key(pos: &Chess, tte: Option<TTEntry>, m: &Move, _g: &SearchState, t: &ThreadState) -> MoveOrderKey {
    // TT-move first
    if let Some(tte) = tte
        && move_match_tt(m, &tte)
    {
        return MoveOrderKey::TTMove(match m.promotion() {
            Some(shakmaty::Role::Queen) => -4,
            Some(shakmaty::Role::Rook) => -3,
            Some(shakmaty::Role::Bishop) => -2,
            Some(shakmaty::Role::Knight) => -1,
            _ => 0,
        });
    }

    if let Some(captured) = m.capture() {
        // for captures, order by MVV-LVA
        let victim_pos = if m.is_en_passant() { Square::from_coords(m.to().file(), m.from().unwrap().rank()) } else { m.to() };
        let victim_value = eval_piece(victim_pos, pos.turn().other(), captured);
        let aggressor_value = eval_piece(
            m.from().unwrap(),
            pos.turn(),
            pos.board().role_at(m.from().unwrap()).unwrap(),
        );
        MoveOrderKey::Capture(-victim_value, -aggressor_value)
    } else {
        let hist = t.butterfly[pos.turn() as usize][m.from().unwrap() as usize][m.to() as usize] as i32;
        MoveOrderKey::Quiet(-hist)
    }
}

// Actual search implementation
struct SearchState<'a> {
    config: &'a crate::Configuration,
    tt: &'a TT,
    nodes: NodeCount,
    deadline: time::Deadline,
    stop: AtomicBool,
}

struct ThreadState<'a> {
    butterfly: &'a mut [[[i16; 64]; 64]; 2],
    pv: [[Option<Move>; 256]; 256],
}

fn qsearch(position: shakmaty::Chess, mut alpha: i16, beta: i16, g: &SearchState, t: &mut ThreadState) -> i16 {
    g.nodes.qnodes.fetch_add(1, Relaxed);

    let (moves, mut best) = if !position.is_check() {
        let best = eval(&position);
        if best >= beta {
            return best;
        }
        if best > alpha {
            alpha = best;
        }
        let mut moves = position.legal_moves();
        moves.retain(|m| m.is_promotion() || m.is_capture());
        (moves, best)
    } else {
        // If checked, search all moves and forbid standing pat
        // Instead, assume checkmate unless a move can let us escape
        (position.legal_moves(), -32700)
    };

    let moves = LazySort::new(&moves, |m| move_key(&position, None, m, g, t));
    for (_i, _key ,mv) in moves {
        let mut pos = position.clone();
        pos.play_unchecked(&mv);
        let score = -qsearch(pos, -beta, -alpha, g, t);
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum NodeType {
    PV,
    Cut,
    All,
}

fn alphabeta(
    position: shakmaty::Chess,
    mut history: Vec<shakmaty::Chess>,
    depth: isize,
    ply: isize,
    mut alpha: i16,
    beta: i16,
    g: &SearchState,
    t: &mut ThreadState,
) -> i16 {
    g.nodes.nodes.fetch_add(1, Relaxed);
    g.nodes.seldepth.fetch_max(ply, Relaxed);
    t.pv[ply as usize][0] = None;

    // Check if we are done; go to qsearch if so
    if depth <= 0 {
        g.nodes.leaves.fetch_add(1, Relaxed);
        return qsearch(position, alpha, beta, g, t);
    }

    // Check if we are out of time
    if g.deadline
        .check_hard(Instant::now(), g.nodes.count() as usize)
        || g.stop.load(Relaxed)
    {
        return -32768;
    }

    // Generate moves; detect checkmate/stalemate
    let moves = position.legal_moves();
    if moves.is_empty() {
        if position.is_check() {
            return -32700;
        } else {
            return 0;
        }
    }

    let mut child_depth = depth - 1;
    if position.is_check() {
        // extend search when in check
        child_depth += 1;
    }

    // Fetch TT entry, do IID if there is none
    let zob: Zobrist64 = position.zobrist_hash(shakmaty::EnPassantMode::Legal);
    let tt_entry = g.tt.get(&moves, zob.0).or_else(|| {
        if depth >= 3 {
            let depth_internal = min(depth - 2, 2);
            alphabeta(
                position.clone(),
                history.clone(),
                depth_internal,
                ply,
                alpha,
                beta,
                g,
                t,
            );
            g.tt.get(&moves, zob.0)
        } else {
            None
        }
    });

    // If we have a valid TT entry, with enough depth, we can potentially use its score (TT-cut)
    if let Some(tte) = tt_entry
        && tte.depth as isize >= depth
    {
        // We can use the TT score for cutoffs, depending on if it's compatible with our alpha/beta window
        // It is compatible if:
        // - it is not an upper bound, and the score >= beta (so the real score also >= beta)
        // - it is not a lower bound, and the score < alpha (so the real score also < alpha)
        let cut = tte.score_type != ScoreType::UpperBound && tte.value >= beta
            || tte.score_type != ScoreType::LowerBound && tte.value <= alpha;

        if cut {
            return tte.value;
        }
    }

    // three-fold repetition draw detection
    let reps = history.iter().filter(|h| **h == position).count();
    if reps >= 2 {
        // draw
        return 0;
    }
    // fifty-move rule draw detection
    if position.halfmoves() >= 100 {
        return 0;
    }
    history.push(position.clone());

    let mut best_value = i16::MIN;
    let mut best_move = moves[0].clone();
    let mut node_type = NodeType::All;
    let mut moves = LazySort::new(&moves, |m| move_key(&position, tt_entry, m, g, t));
    while let Some((_i, _key, mv)) = moves.next() {
        let mut pos = position.clone();
        pos.play_unchecked(mv);
        let hist = if mv.is_zeroing() { Vec::new() } else { history.clone() };

        let score = alphabeta(pos, hist, child_depth, ply + 1, -beta, -alpha, g, t);
        if score == -32768 {
            // out of time
            return score;
        }
        let score = -score;
        if score > best_value {
            best_value = score;
            best_move = mv.clone();
            if score >= beta {
                // fail-soft
                node_type = NodeType::Cut;

                // Update butterfly table
                if !mv.is_capture() {
                    const MAX_HISTORY: i32 = 16384;
                    let bonus = (300 * depth as i32 - 250).clamp(-MAX_HISTORY, MAX_HISTORY);
                    let col = position.turn() as usize;
                    let from = mv.from().unwrap() as usize;
                    let to = mv.to() as usize;
                    t.butterfly[col][from][to] += (bonus - (t.butterfly[col][from][to] as i32 * bonus.abs()) / (MAX_HISTORY)) as i16;

                    for fail in moves.seen().filter(|m| !m.is_capture() && *m != mv) {
                        let from = fail.from().unwrap() as usize;
                        let to = fail.to() as usize;
                        t.butterfly[col][from][to] -= (bonus - (t.butterfly[col][from][to] as i32 * bonus.abs()) / (MAX_HISTORY)) as i16;
                    }
                }

                break;
            }
            if score > alpha {
                alpha = score;
                node_type = NodeType::PV;
                t.pv[ply as usize][0] = Some(mv.clone());
                for i in 0..255 {
                    t.pv[ply as usize][i + 1] = t.pv[ply as usize + 1][i].clone();
                }
            }

        }
    }

    if best_value < -32500 {
        best_value += 1;
    }
    if best_value > 32500 {
        best_value -= 1;
    }
    g.tt.write(
        zob.0,
        TTEntry {
            from: best_move.from().unwrap() as u8,
            to: best_move.to() as u8,
            depth: depth as u8,
            value: best_value,
            score_type: match node_type {
                NodeType::PV => ScoreType::Exact,
                NodeType::Cut => ScoreType::LowerBound,
                NodeType::All => ScoreType::UpperBound,
            },
        },
    );
    return best_value;
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

fn collect_pv(t: &ThreadState) -> Vec<Move> {
    let mut pv = Vec::new();
    for i in 0..256 {
        match &t.pv[0][i] {
            Some(mv) => pv.push(mv.clone()),
            None => break,
        }
    }
    pv
}

pub fn search(
    position: shakmaty::Chess,
    history: Vec<shakmaty::Chess>,
    deadline: time::Deadline,
    tt: &TT,
    config: &crate::Configuration,
    butterfly: &mut [[[i16; 64]; 64]; 2],
    callback: &mut dyn FnMut(isize, ruci::Score, &Vec<Move>, &NodeCount),
) -> (ruci::Score, Vec<Move>, NodeCount) {
    let mut score = eval(&position);
    let mut pv = Vec::new();
    let global = SearchState {
        config,
        tt,
        nodes: NodeCount {
            nodes: AtomicU64::new(0),
            leaves: AtomicU64::new(0),
            qnodes: AtomicU64::new(0),
            seldepth: AtomicIsize::new(0),
        },
        deadline,
        stop: AtomicBool::new(false),
    };
    let mut local = ThreadState {
        butterfly: butterfly,
        pv: std::array::from_fn(|_| std::array::from_fn(|_| None)),
    };
    for d in 1.. {
        let alpha = score - 50;
        let beta = score + 50;
        let asp_score = alphabeta(position.clone(), history.clone(), d, 0, alpha, beta, &global, &mut local);
        let new_score = if asp_score > alpha && asp_score < beta {
            asp_score 
        } else {
            alphabeta(position.clone(), history.clone(), d, 0, i16::MIN + 1, i16::MAX - 1, &global, &mut local)
        };
        if new_score == -32768 {
            // out of time
            callback(65535, convert_score(score), &pv, &global.nodes);
            break;
        }
        pv = collect_pv(&local);
        score = new_score;
        callback(d, convert_score(score), &pv, &global.nodes);
        if !pv.is_empty()
            && global.deadline.check_soft(
                Instant::now(),
                (global.nodes.count()) as usize,
                d as usize,
            )
        {
            break;
        }
    }

    (convert_score(score), pv, global.nodes)
}
