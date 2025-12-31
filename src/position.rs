use shakmaty::{
    CastlingSide, Chess, Color, Move, zobrist::{Zobrist64, ZobristHash, ZobristValue}
};

type Zob = Zobrist64;
pub struct Position {
    pos: Chess,
    zobrist: Zobrist64,
    // TODO: add more fields as necessary, e.g. NNUE accumulators
}

impl Position {
    pub fn new(pos: Chess) -> Self {
        // let zobrist = shakmaty::zobrist::hash(&pos);
        let zobrist = pos.zobrist_hash(shakmaty::EnPassantMode::Legal);
        Position { pos, zobrist }
    }

    pub fn pos(&self) -> &Chess {
        &self.pos
    }

    pub fn zobrist(&self) -> u64 {
        self.zobrist()
    }
}

impl shakmaty::Position for Position {
    fn board(&self) -> &shakmaty::Board {
        self.pos.board()
    }

    fn promoted(&self) -> shakmaty::Bitboard {
        self.pos.promoted()
    }

    fn pockets(&self) -> Option<&shakmaty::ByColor<shakmaty::ByRole<u8>>> {
        self.pos.pockets()
    }

    fn turn(&self) -> shakmaty::Color {
        self.pos.turn()
    }

    fn castles(&self) -> &shakmaty::Castles {
        self.pos.castles()
    }

    fn maybe_ep_square(&self) -> Option<shakmaty::Square> {
        self.pos.maybe_ep_square()
    }

    fn remaining_checks(&self) -> Option<&shakmaty::ByColor<shakmaty::RemainingChecks>> {
        self.pos.remaining_checks()
    }

    fn halfmoves(&self) -> u32 {
        self.pos.halfmoves()
    }

    fn fullmoves(&self) -> std::num::NonZeroU32 {
        self.pos.fullmoves()
    }

    fn into_setup(self, mode: shakmaty::EnPassantMode) -> shakmaty::Setup {
        self.pos.into_setup(mode)
    }

    fn legal_moves(&self) -> shakmaty::MoveList {
        self.pos.legal_moves()
    }

    fn is_variant_end(&self) -> bool {
        self.pos.is_variant_end()
    }

    fn has_insufficient_material(&self, color: shakmaty::Color) -> bool {
        self.pos.has_insufficient_material(color)
    }

    fn variant_outcome(&self) -> Option<shakmaty::Outcome> {
        self.pos.variant_outcome()
    }

    fn play_unchecked(&mut self, m: &Move) {
        // Update zobrist hash
        self.zobrist ^= Zob::zobrist_for_white_turn();

        // Clear epsquare zobrist
        if let Some(sq) = self.pos.ep_square(shakmaty::EnPassantMode::Always) {
            self.zobrist ^= Zob::zobrist_for_en_passant_file(sq.file());
        }

        // Clear castling zobrist
        let castles = self.pos.castles();
        for color in Color::ALL {
            for side in CastlingSide::ALL {
                if castles.has(color, side) {
                    self.zobrist ^= Zob::zobrist_for_castling_right(color, side);
                }
            }
        }

        // NOTE: crazyhouse and three-check require more zobrist updates

        match m {
            Move::Normal { role, from, capture, to, promotion } => {
                // Remove piece from 'from' square
                let piece = self.pos.board().piece_at(*from).unwrap();
                self.zobrist ^= Zob::zobrist_for_piece(*from, piece);

                // If capture, remove captured piece from 'to' square
                if let Some(role) = capture {
                    let color = piece.color.other();
                    let captured_piece = shakmaty::Piece { role: *role, color };
                    self.zobrist ^= Zob::zobrist_for_piece(*to, captured_piece);
                }

                // Add piece to 'to' square (with promotion if applicable)
                let moved_piece = if let Some(role) = promotion {
                    shakmaty::Piece { role: *role, color: piece.color }
                } else {
                    piece
                };
                self.zobrist ^= Zob::zobrist_for_piece(*to, moved_piece);
            },
            Move::EnPassant { from, to } => todo!(),
            Move::Castle { king, rook } => todo!(),
            Move::Put { role, to } => todo!(),
        }

        self.pos.play_unchecked(m);

        // Set epsquare zobrist
        if let Some(sq) = self.pos.ep_square(shakmaty::EnPassantMode::Always) {
            self.zobrist ^= Zob::zobrist_for_en_passant_file(sq.file());
        }

        // Set castling zobrist
        let castles = self.pos.castles();
        for color in Color::ALL {
            for side in CastlingSide::ALL {
                if castles.has(color, side) {
                    self.zobrist ^= Zob::zobrist_for_castling_right(color, side);
                }
            }
        }

    }
}
