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
use std::sync::atomic::AtomicU64;

mod bench;
mod search;
// mod position;
mod eval;
mod time;

struct State {
    position: Chess,
    history: Vec<Chess>,
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
        history: Vec::new(),
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

                match moves.iter().try_fold((position, Vec::new()), |(mut position, mut history), r#move| {
                    history.push(position.clone());
                    let r#move = r#move.to_move(&position)?;
                    position.play_unchecked(&r#move);
                    Ok::<_, IllegalUciMoveError>((position, history))
                }) {
                    Ok((position, history)) => {
                        state = State { position, history };
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

                let tc = time::TimeControl::from_ruci(state.position.turn(), &go);
                let deadline = match tc {
                    Some(tc) => time::Deadline::from_tc(&tc, std::time::Instant::now()),
                    None => time::Deadline::Depth(6),
                };

                let starttime = std::time::Instant::now();
                let mut tt = Vec::new(); tt.resize_with(1 << 20, || AtomicU64::new(0));
                let (_score, mut pv, _count) = search::search(
                    state.position.clone(), 
                    state.history.clone(),
                    deadline,
                    &mut tt,
                    &mut |depth, score, pv, count| {
                        let elapsed = starttime.elapsed().as_millis() as u64;
                        let nodes = count.nodes + count.qnodes - count.leaves;
                        let nps = nodes * 1000 / elapsed.max(1);
                        let info = Info {
                            depth: Some(Depth {
                                depth: depth as usize,
                                seldepth: None,
                            }),
                            pv: Cow::Owned(pv.iter().rev().map(|m| m.to_uci(CastlingMode::Standard)).collect()),
                            score: Some(ruci::ScoreWithBound {
                                kind: score,
                                bound: None,
                            }),
                            nodes: Some(nodes as usize),
                            nps: Some(nps as usize),
                            time: Some(elapsed as usize),
                            ..Default::default()
                        };
                        gui.send(info).unwrap();
                    }
                );
                pv.reverse();
                let bestmove = pv.first().cloned();
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
    if std::env::args().any(|arg| arg == "bench") {
        bench::bench();
        return;
    }

    engine(stdout().lock(), stdin().lock()).unwrap();
}