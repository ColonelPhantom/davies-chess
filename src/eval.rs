use shakmaty::{Color, Position, Role, Square};

// Taken from https://www.chessprogramming.org/Simplified_Evaluation_Function
const PST: [[i16; 64]; 6] = [
    // pawn
    [
         0,  0,  0,  0,  0,  0,  0,  0,
        50, 50, 50, 50, 50, 50, 50, 50,
        10, 10, 20, 30, 30, 20, 10, 10,
         5,  5, 10, 25, 25, 10,  5,  5,
         0,  0,  0, 20, 20,  0,  0,  0,
         5, -5,-10,  0,  0,-10, -5,  5,
         5, 10, 10,-20,-20, 10, 10,  5,
         0,  0,  0,  0,  0,  0,  0,  0
    ],

    // knight
    [
        -50,-40,-30,-30,-30,-30,-40,-50,
        -40,-20,  0,  0,  0,  0,-20,-40,
        -30,  0, 10, 15, 15, 10,  0,-30,
        -30,  5, 15, 20, 20, 15,  5,-30,
        -30,  0, 15, 20, 20, 15,  0,-30,
        -30,  5, 10, 15, 15, 10,  5,-30,
        -40,-20,  0,  5,  5,  0,-20,-40,
        -50,-40,-30,-30,-30,-30,-40,-50,
    ],

    // bishop
    [
        -20,-10,-10,-10,-10,-10,-10,-20,
        -10,  0,  0,  0,  0,  0,  0,-10,
        -10,  0,  5, 10, 10,  5,  0,-10,
        -10,  5,  5, 10, 10,  5,  5,-10,
        -10,  0, 10, 10, 10, 10,  0,-10,
        -10, 10, 10, 10, 10, 10, 10,-10,
        -10,  5,  0,  0,  0,  0,  5,-10,
        -20,-10,-10,-10,-10,-10,-10,-20,
    ],

    // rook
    [
         0,  0,  0,  0,  0,  0,  0,  0,
         5, 10, 10, 10, 10, 10, 10,  5,
        -5,  0,  0,  0,  0,  0,  0, -5,
        -5,  0,  0,  0,  0,  0,  0, -5,
        -5,  0,  0,  0,  0,  0,  0, -5,
        -5,  0,  0,  0,  0,  0,  0, -5,
        -5,  0,  0,  0,  0,  0,  0, -5,
         0,  0,  0,  5,  5,  0,  0,  0
    ],

    //queen
    [
        -20,-10,-10, -5, -5,-10,-10,-20,
        -10,  0,  0,  0,  0,  0,  0,-10,
        -10,  0,  5,  5,  5,  5,  0,-10,
         -5,  0,  5,  5,  5,  5,  0, -5,
          0,  0,  5,  5,  5,  5,  0, -5,
        -10,  5,  5,  5,  5,  5,  0,-10,
        -10,  0,  5,  0,  0,  0,  0,-10,
        -20,-10,-10, -5, -5,-10,-10,-20
    ],

    // king middle game
    [
        -30,-40,-40,-50,-50,-40,-40,-30,
        -30,-40,-40,-50,-50,-40,-40,-30,
        -30,-40,-40,-50,-50,-40,-40,-30,
        -30,-40,-40,-50,-50,-40,-40,-30,
        -20,-30,-30,-40,-40,-30,-30,-20,
        -10,-20,-20,-20,-20,-20,-20,-10,
         20, 20,  0,  0,  0,  0, 20, 20,
         20, 30, 10,  0,  0, 10, 30, 20
    ],
];

pub fn eval_piece(sq: Square, color: Color, role: Role) -> i16 {
    let base_piece_value = match role {
        shakmaty::Role::Pawn => 100,
        shakmaty::Role::Knight => 320,
        shakmaty::Role::Bishop => 330,
        shakmaty::Role::Rook => 500,
        shakmaty::Role::Queen => 900,
        shakmaty::Role::King => 0, // both sides have 1 king always
    };

    let piece_idx: usize = role.into();
    let sq_idx: usize = if color == shakmaty::Color::White {
        sq.flip_vertical().into()
    } else {
        sq.into()
    };

    let pst_value = PST[piece_idx - 1][sq_idx];
    base_piece_value + pst_value
}

#[inline(never)]
pub fn eval(position: &shakmaty::Chess) -> i16 {
    // Simple material evaluation
    let mut score = 0;

    for (sq, piece) in position.board() {
        let piece_value = eval_piece(sq, piece.color, piece.role);

        if piece.color == position.turn() {
            score += piece_value;
        } else {
            score -= piece_value;
        }
    }

    // let white_pawns = position.board().pawns() & position.board().by_color(Color::White);
    // let black_pawns = position.board().pawns() & position.board().by_color(Color::Black);
    // let white_blockers = white_pawns.shift(8) & position.board().by_color(Color::Black);
    // let black_blockers = black_pawns.shift(-8) & position.board().by_color(Color::White);

    // score -= 35 * white_blockers.count() as i16;
    // score += 35 * black_blockers.count() as i16;

    // let mobility = position.legal_moves().len() as i16;
    // let opp_mobility = position.clone().swap_turn().map(|p| p.legal_moves().len() as i16).unwrap_or(0);

    // score += 5 * mobility;
    // score -= 5 * opp_mobility;

    score
}