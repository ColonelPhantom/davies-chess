use std::sync::atomic::AtomicU64;

use shakmaty::Chess;

use crate::{search::search, time};

const POSITIONS: [(&str, isize); 3] = [
    ("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", 7),
    ("r1bq1rk1/4ppbp/p1pp1np1/1P2n3/2B1PB2/2NP1N1P/1PP2PP1/R2QR1K1 b - - 0 11", 7),
    ("8/k7/3p4/p2P1p2/P2P1P2/8/8/K7 w - ", 8),
];

pub fn bench() {
    for (fen, depth) in POSITIONS {
        let fen: shakmaty::fen::Fen = fen.parse().unwrap();
        let position: Chess = fen
            .clone()
            .into_position(shakmaty::CastlingMode::Standard)
            .unwrap();
        let mut tt = Vec::new();
        tt.resize_with(1 << 24, || AtomicU64::new(0));
        let (score, pv, count) = search(
            position,
            Vec::new(),
            time::Deadline::Depth(depth as usize),
            &mut tt,
            &mut |_, _, _, _| {},
        );
        println!("FEN: {}", fen);
        println!(
            "Depth: {}, Score: {:?}, Nodes: {}, Leaves: {}, QNodes: {}, Total: {}",
            depth,
            score,
            count.nodes,
            count.leaves,
            count.qnodes,
            count.nodes + count.qnodes - count.leaves
        );
        println!("Principal Variation:");
        for mv in pv.iter().rev() {
            print!("{} ", mv);
        }
        println!("\n");
    }
}
