use shakmaty::Chess;

use crate::{search::search, time};

const POSITIONS: [(&str, isize); 7] = [
    ("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", 8),
    ("r1bq1rk1/4ppbp/p1pp1np1/1P2n3/2B1PB2/2NP1N1P/1PP2PP1/R2QR1K1 b - - 0 11", 7),
    ("2r3r1/3R2pk/p1p1PB2/1pR2P2/2p1PK2/P1P5/8/5b2 w - - 9 19", 7),
    ("2R1b3/6pk/p3P3/5P2/1Pp2K2/2P5/8/8 b - - 0 28", 10),
    ("r1b1kb1r/ppp2ppp/4pn2/3q4/1n1P4/5NP1/PP2PP1P/RNBQKB1R b KQkq - 4 8", 6),
    ("8/k7/3p4/p2P1p2/P2P1P2/8/8/K7 w - ", 30),
    ("Q4QR1/1p5p/k1p5/p7/3K4/8/P7/8 b - - 2 56", 4),
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
            &crate::DEFAULT_CONFIG,
            &mut [[[0; 64]; 64]; 2],
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
