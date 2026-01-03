use std::{
    cmp::min,
    sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
    time::Instant,
};

use crate::{
    eval::{eval, eval_piece},
    time,
    util::sort::LazySort,
};
use shakmaty::{
    Chess, Move, Position,
    zobrist::{Zobrist64, ZobristHash},
};

pub struct NodeCount {
    pub nodes: AtomicU64,
    pub leaves: AtomicU64,
    pub qnodes: AtomicU64,
}

impl NodeCount {
    pub fn count(&self) -> u64 {
        self.nodes.load(Relaxed) + self.qnodes.load(Relaxed) - self.leaves.load(Relaxed)
    }
}

// Move ordering
// note: somewhat confusing, but for the inner values, lower is better
// this is related to how sorting works (lower values earlier)
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum MoveOrderKey {
    TTMove(i16),
    Capture(i16, i16), // victim value, aggressor value
    Quiet(i16),        // development value
}

fn move_match_tt(m: &Move, tte: &TTEntry) -> bool {
    (m.from().unwrap() as u8 == tte.from) && (m.to() as u8 == tte.to)
}

fn move_key(pos: &Chess, tte: Option<TTEntry>, m: &Move) -> MoveOrderKey {
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
        let victim_value = eval_piece(m.to(), pos.turn().other(), captured);
        let aggressor_value = eval_piece(
            m.from().unwrap(),
            pos.turn(),
            pos.board().role_at(m.from().unwrap()).unwrap(),
        );
        MoveOrderKey::Capture(-victim_value, -aggressor_value)
    } else {
        // for quiet moves, order by piece development (looking at PSQTs)
        let role = pos.board().role_at(m.from().unwrap()).unwrap();
        let role_to = m.promotion().unwrap_or(role);
        let value_old = eval_piece(m.from().unwrap(), pos.turn(), role);
        let value_new = eval_piece(m.to(), pos.turn(), role_to);
        let devel = value_new - value_old;
        MoveOrderKey::Quiet(-devel)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScoreType {
    Exact = 0,
    LowerBound = 1,
    UpperBound = 2,
}

// Transposition table
// TT-Entry bitmap:
// 24 bits: high part of zobrist key (useful up to 2^40 entries, after that some bits become redundant with index; maybe use buckets at some point?)
// 8 bits: search depth
// 16 bits: score
// 6 bits: from square
// 6 bits: to square
// 2 bits: score type
// 2 bits: free!

#[derive(Clone, Copy)]
struct TTEntry {
    // basic best-move info
    from: u8,
    to: u8,

    // TODO: value info etc
    value: i16,
    depth: u8,
    score_type: ScoreType,
}

fn get_tt(tt: &Vec<AtomicU64>, moves: &[Move], key: u64) -> Option<TTEntry> {
    let index = (key % tt.len() as u64) as usize;
    let entry = tt[index].load(std::sync::atomic::Ordering::Relaxed);
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

fn write_tt(tt: &Vec<AtomicU64>, key: u64, data: TTEntry) {
    let index = (key % tt.len() as u64) as usize;
    let entry = (key & 0xFFFFFF0000000000) // basically << 40 but keeping the high part
        | ((data.depth as u64) << 32)
        | ((data.value.cast_unsigned() as u64) << 16)
        | ((data.from as u64) << 10)
        | ((data.to as u64) << 4)
        | ((data.score_type as u64) << 2);
    tt[index].store(entry, std::sync::atomic::Ordering::Relaxed);
}

// Actual search implementation
struct SearchState {
    tt: Vec<AtomicU64>,
    nodes: NodeCount,
    deadline: time::Deadline,
    stop: AtomicBool,
}

fn qsearch(position: shakmaty::Chess, mut alpha: i16, beta: i16, global: &SearchState) -> i16 {
    global.nodes.qnodes.fetch_add(1, Relaxed);

    let (moves, mut best) = if !position.is_check() {
        let best = eval(&position);
        if best >= beta {
            return best;
        }
        if best > alpha {
            alpha = best;
        }
        (position.capture_moves(), best)
    } else {
        // If checked, search all moves and forbid standing pat
        // Instead, assume checkmate unless a move can let us escape
        (position.legal_moves(), -32700)
    };

    let moves = LazySort::new(&moves, |m| move_key(&position, None, m));
    for mv in moves {
        let mut pos = position.clone();
        pos.play_unchecked(&mv);
        let score = -qsearch(pos, -beta, -alpha, global);
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
    mut alpha: i16,
    beta: i16,
    g: &SearchState,
) -> (i16, Vec<Move>) {
    g.nodes.nodes.fetch_add(1, Relaxed);

    // Check if we are done; go to qsearch if so
    if depth <= 0 {
        g.nodes.leaves.fetch_add(1, Relaxed);
        return (qsearch(position, alpha, beta, g), Vec::new());
    }

    // Check if we are out of time
    if g.deadline
        .check_hard(Instant::now(), g.nodes.count() as usize)
        || g.stop.load(Relaxed)
    {
        return (-32768, Vec::new());
    }

    // Generate moves; detect checkmate/stalemate
    let moves = position.legal_moves();
    if moves.is_empty() {
        if position.is_check() {
            return (-32700, Vec::new());
        } else {
            return (0, Vec::new());
        }
    }

    // Fetch TT entry, do IID if there is none
    let zob: Zobrist64 = position.zobrist_hash(shakmaty::EnPassantMode::Legal);
    let tt_entry = get_tt(&g.tt, &moves, zob.0).or_else(|| {
        if depth >= 3 {
            let depth_internal = min(depth - 2, 2);
            alphabeta(
                position.clone(),
                history.clone(),
                depth_internal,
                alpha,
                beta,
                g,
            );
            get_tt(&g.tt, &moves, zob.0)
        } else {
            None
        }
    });

    // If we have a valid TT entry, with enough depth, we can potentially use its score (TT-cut)
    if let Some(tte) = tt_entry
        && tte.depth as isize >= depth
    {
        // We can use the TT score, depending on if it's compatible with our alpha/beta window
        // TODO: fix pv handling for tt-cutoffs
        match tte.score_type {
            ScoreType::Exact => {
                // TODO: this cuts off PV, and might be wrong?
                return (tte.value, Vec::new());
            }
            ScoreType::LowerBound if tte.value >= beta => {
                // We know that at least one move is better than beta; cut
                return (tte.value, Vec::new());
            }
            ScoreType::UpperBound if tte.value < alpha => {
                // We know that no moves will raise alpha; cut
                return (tte.value, Vec::new());
            }
            _ => {}
        }
    }

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

    let mut pv = Vec::new();
    let mut best_value = i16::MIN;
    let mut best_move = moves[0].clone();
    let mut node_type = NodeType::All;
    let moves = LazySort::new(&moves, |m| move_key(&position, tt_entry, m));
    for mv in moves {
        let mut pos = position.clone();
        pos.play_unchecked(mv);
        let hist = if mv.is_zeroing() { Vec::new() } else { history.clone() };

        let (mut score, mut sub_pv);
        // TODO: what if we don't expect to find a raising move?
        if node_type == NodeType::All {
            (score, sub_pv) = alphabeta(pos, hist, depth - 1, -beta, -alpha, g);
        } else {
            (score, sub_pv) = alphabeta(pos.clone(), hist.clone(), depth - 1, -alpha - 1, -alpha, g);
            if -score > alpha && beta - alpha > 1 {
                (score, sub_pv) = alphabeta(pos, hist, depth - 1, -beta, -alpha, g);
            }
        }
        if score == -32768 {
            // out of time
            return (-32768, Vec::new());
        }
        let score = -score;
        if score > best_value {
            best_value = score;
            best_move = mv.clone();
            if score > alpha {
                alpha = score;
                pv = sub_pv;
                pv.push(mv.clone());
                node_type = NodeType::PV;
            }
            if score >= beta {
                // fail-soft
                node_type = NodeType::Cut;
                break;
            }
        }
    }

    write_tt(
        &g.tt,
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
    if best_value < -32500 {
        best_value += 1;
    }
    if best_value > 32500 {
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
    tt: Vec<AtomicU64>,
    callback: &mut dyn FnMut(isize, ruci::Score, &Vec<Move>, &NodeCount),
) -> (ruci::Score, Vec<Move>, NodeCount) {
    let mut score = eval(&position);
    let mut pv = Vec::new();
    let global = SearchState {
        tt,
        nodes: NodeCount {
            nodes: AtomicU64::new(0),
            leaves: AtomicU64::new(0),
            qnodes: AtomicU64::new(0),
        },
        deadline,
        stop: AtomicBool::new(false),
    };
    for d in 0.. {
        let asp_alpha = score - 50;
        let asp_beta = score + 50;
        let (asp_score, asp_pv) = alphabeta(
            position.clone(),
            history.clone(),
            d,
            asp_alpha,
            asp_beta,
            &global,
        );
        if asp_score == -32768 {
            // out of time
            callback(65534, convert_score(score), &pv, &global.nodes);
            break;
        }
        let (new_score, new_pv) = if asp_score > asp_alpha && asp_score < asp_beta {
            (asp_score, asp_pv)
        } else {
            // TODO: what if search fails a second time?
            #[rustfmt::skip]
            let alpha = if asp_score <= asp_alpha { i16::MIN + 1 } else { asp_alpha };
            #[rustfmt::skip]
            let beta = if asp_score >= asp_beta { i16::MAX - 1 } else { asp_beta };
            let (sc, pv) = alphabeta(position.clone(), history.clone(), d, alpha, beta, &global);
            if sc == -32768 {
                // out of time
                callback(65535, convert_score(score), &pv, &global.nodes);
                break;
            }
            (sc, pv)
        };
        score = new_score;
        pv = new_pv;
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
