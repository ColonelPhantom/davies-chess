use crate::eval::{eval};
use shakmaty::Position;

pub fn qsearch(
    position: shakmaty::Chess,
    mut alpha: i16,
    beta: i16,
) -> i16 {
    let eval = eval(&position);
    let mut best = eval;
    if best >= beta {
        return best;
    }
    if best > alpha {
        alpha = best;
    }
    let moves = position.capture_moves();
    // TODO: move ordering, use SEE-pruning
    for mv in moves {
        let mut pos = position.clone();
        pos.play_unchecked(&mv);
        let score = -qsearch(pos, -beta, -alpha);
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

pub fn alphabeta(
    position: shakmaty::Chess,
    depth: isize,
    mut alpha: i16,
    beta: i16,
) -> (i16, Vec<shakmaty::Move>) {
    if depth <= 0 {
        // TODO: add qsearch
        return (qsearch(position, alpha, beta), Vec::new());
    }

    let moves = position.legal_moves();
    if moves.is_empty() {
        if position.is_check() {
            return (-10000 + (10 - depth) as i16, Vec::new());
        } else {
            return (0, Vec::new());
        }
    }

    let mut pv = Vec::new();
    let mut best_value = i16::MIN;
    for mv in moves {
        let mut pos = position.clone();
        pos.play_unchecked(&mv);
        let (score, sub_pv) = alphabeta(pos, depth - 1, -beta, -alpha);
        let score = -score;
        if score > best_value {
            best_value = score;
            if score > alpha {
                alpha = score;
                pv = sub_pv;
                pv.push(mv);
            }
        }
        if score >= beta {
            // fail-soft
            return (best_value, pv);
        }
    }

    return (best_value, pv);
}
