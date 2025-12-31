//! This example shows how to make a "portable" engine, which can easily be used in various
//! I/O situations.
//!
//! - See `engine-stdio` for an implementation using [`stdin`](io::stdin) and [`stdout`](io::stdout).
//! - See `engine-server` for a TCP stream implementation.
//!
//! # Specifications
//! All communication is done with UCI, using the [`Info`] message when another message is not
//! more appropriate.
//!
//! Accepts the following messages:
//! - [`Uci`](ruci::Uci)
//! - [`Position`](ruci::Position)
//! - [`Go`](ruci::Go) - no analysis, just outputs the first legal move [`shakmaty`] finds.
//!   Parameters are ignored except [`infinite`](ruci::Go#structfield.infinite).
//! - [`Quit`](ruci::Quit)

use ruci::gui::Message;
use ruci::{BestMove, Depth, Gui, Id, Info, NormalBestMove, UciOk, ReadyOk};
use shakmaty::uci::{IllegalUciMoveError, UciMove};
use shakmaty::{CastlingMode, Chess, Position};
use std::borrow::Cow;
use std::io::{self, stdin, stdout};
use std::io::{BufRead, Write};
use std::thread::sleep;
use std::time::Duration;

mod search;
// mod position;
mod eval;

struct State {
    position: Chess,
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Starts a new engine that forever reads messages, unless told to quit.
pub fn engine<E, G>(engine: E, gui: G) -> io::Result<()>
where
    E: Write,
    G: BufRead,
{
    let mut gui = Gui { engine, gui };
    let mut state = State {
        position: Chess::new(),
    };

    gui.send_string("engine started")?;

    loop {
        let message = gui.read();

        let message = match message {
            Ok(m) => m,
            Err(e) => {
                gui.send_string(&e.to_string())?;
                continue;
            }
        };

        match message {
            Message::Quit(_) => return Ok(()),
            Message::Position(position) => {
                let (position, moves) = match position {
                    ruci::Position::StartPos { moves } => (Chess::new(), moves),
                    ruci::Position::Fen { moves, fen } => {
                        match fen.into_owned().into_position(CastlingMode::Standard) {
                            Ok(p) => (p, moves),
                            Err(e) => {
                                gui.send_string(&format!("error parsing FEN: {e}"))?;
                                continue;
                            }
                        }
                    }
                };

                match moves.iter().try_fold(position, |mut position, r#move| {
                    let r#move = r#move.to_move(&state.position)?;
                    position.play_unchecked(&r#move);
                    Ok::<Chess, IllegalUciMoveError>(position)
                }) {
                    Ok(position) => {
                        state.position = position;
                        gui.send_string("position set")?;
                    }
                    Err(e) => {
                        gui.send_string(&format!("error converting UCI move to valid move: {e}"))?;
                    }
                }
            }
            Message::Go(go) => {
                if state.position.legal_moves().first().is_none() {
                    let null = BestMove::Normal(NormalBestMove {
                        r#move: UciMove::Null,
                        ponder: None,
                    });
                    gui.send(null)?;
                }

                let depth = go.depth.unwrap_or(6);
                let mut bestmove = None;
                for d in 1..=depth {
                    let (score, mut pv) = search::alphabeta(
                        state.position.clone(), 
                        d as isize, 
                        -32000, 
                        32000
                    );
                    pv.reverse();
                    bestmove = pv.first().cloned();
                    let uci_pv: Vec<_> = pv.iter().map(|m| m.to_uci(CastlingMode::Standard)).collect();
                    let info = Info {
                        depth: Some(Depth {
                            depth: d,
                            seldepth: None,
                        }),
                        pv: Cow::Owned(uci_pv),
                        score: Some(ruci::ScoreWithBound {
                            kind: ruci::Score::Centipawns(score as isize),
                            bound: None,
                        }),
                        ..Default::default()
                    };
                    gui.send(info)?;
                }
                if let Some(mv) = bestmove {
                    let best_move = BestMove::Normal(NormalBestMove {
                        r#move: mv.to_uci(CastlingMode::Standard),
                        ponder: None,
                    });
                    gui.send(best_move)?;
                } else {
                    let null = BestMove::Normal(NormalBestMove {
                        r#move: UciMove::Null,
                        ponder: None,
                    });
                    gui.send(null)?;
                }
            }
            Message::Uci(_) => {
                let name = format!("Davies {}", VERSION);
                let id_name = Id::Name (Cow::Borrowed(&name));
                let id_author = Id::Author(Cow::Borrowed("Quinten Kock"));

                gui.send(id_name)?;
                gui.send(id_author)?;
                gui.send(UciOk)?;
            }
            Message::IsReady(_) => {
                gui.send(ReadyOk)?;
            }
            _ => gui.send_string("unsupported message")?,
        }
    }
}

pub fn main() {
    engine(stdout().lock(), stdin().lock()).unwrap();
}