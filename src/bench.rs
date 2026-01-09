use shakmaty::Chess;

use crate::{search::search, time};

const POSITIONS: [(&str, isize); 3] = [
    ("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", 8),
    ("r1bq1rk1/4ppbp/p1pp1np1/1P2n3/2B1PB2/2NP1N1P/1PP2PP1/R2QR1K1 b - - 0 11", 8),
    ("8/k7/3p4/p2P1p2/P2P1P2/8/8/K7 w - ", 30),
];

pub fn bench() {
    let start = std::time::Instant::now();
    let mut total_nodes = 0u64;
    for (fen, depth) in POSITIONS {
        let start_this = std::time::Instant::now();
        let fen: shakmaty::fen::Fen = fen.parse().unwrap();
        let position: Chess = fen
            .clone()
            .into_position(shakmaty::CastlingMode::Standard)
            .unwrap();
        let tt = crate::search::tt::TT::new(1 << 24);
        let (score, _pv, count) = search(
            position,
            Vec::new(),
            time::Deadline::Depth(depth as usize),
            &tt,
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
        println!("Time elapsed: {:?}", start_this.elapsed());
        total_nodes += count.count();
    }
    println!(
        "Total time elapsed: {:?}, Total nodes: {}, NPS: {}",
        start.elapsed(),
        total_nodes,
        total_nodes as u128 * 1000 / start.elapsed().as_millis().max(1)
    );
}
