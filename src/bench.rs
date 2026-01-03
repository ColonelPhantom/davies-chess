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
        let tt = crate::search::tt::TT::new(1 << 20);
        let (score, pv, count) = search(
            position,
            Vec::new(),
            time::Deadline::Depth(depth as usize),
            tt,
            &mut |_, _, _, _| {},
        );
        println!("FEN: {}", fen);
        println!(
            "Depth: {}, Score: {:?}, Nodes: {}, Leaves: {}, QNodes: {}, Total: {}",
            depth,
            score,
            count.nodes.load(std::sync::atomic::Ordering::Relaxed),
            count.leaves.load(std::sync::atomic::Ordering::Relaxed),
            count.qnodes.load(std::sync::atomic::Ordering::Relaxed),
            count.count(),
        );
        print!("PV:");
        for mv in pv.iter().rev() {
            print!("{} ", mv);
        }
        println!("\n");
    }
}
