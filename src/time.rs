use std::time::Instant;

pub enum TimeControl {
    FixedDepth(usize),
    FixedNodes(usize),
    FixedTime(usize),
    Infinite,
    Clock {
        time_ms: usize,
        increment_ms: usize,
        moves_to_go: Option<usize>,
    }
}

impl TimeControl {
    pub fn from_ruci(side: shakmaty::Color, tc: &ruci::Go) -> Option<TimeControl> {
        if tc.infinite {
            Some(TimeControl::Infinite)
        } else if let Some(depth) = tc.depth {
            Some(TimeControl::FixedDepth(depth))
        } else if let Some(nodes) = tc.nodes {
            Some(TimeControl::FixedNodes(nodes))
        } else if let Some(time) = tc.move_time {
            Some(TimeControl::FixedTime(time))
        } else if let Some(wtime) = tc.w_time && side == shakmaty::Color::White {
            Some(TimeControl::Clock {
                time_ms: wtime,
                increment_ms: tc.w_inc.map(|x| x.get()).unwrap_or(0),
                moves_to_go: tc.moves_to_go.map(|x| x.get()),
            })
        } else if let Some(btime) = tc.b_time && side == shakmaty::Color::Black {
            Some(TimeControl::Clock {
                time_ms: btime,
                increment_ms: tc.b_inc.map(|x| x.get()).unwrap_or(0),
                moves_to_go: tc.moves_to_go.map(|x| x.get()),
            })
        } else {
            None
        }
    }
}

pub enum Deadline {
    Depth(usize),
    Nodes(usize),
    Time(Instant, Instant), // soft & hard deadlines
    None,
}

impl Deadline {
    pub fn from_tc(tc: &TimeControl, start: Instant) -> Deadline {
        match tc {
            TimeControl::FixedDepth(d) => Deadline::Depth(*d),
            TimeControl::FixedNodes(n) => Deadline::Nodes(*n),
            TimeControl::FixedTime(t) => {
                let soft = start + std::time::Duration::from_millis((*t as u64) / 2);
                let hard = start + std::time::Duration::from_millis(*t as u64);
                Deadline::Time(soft, hard)
            },
            TimeControl::Infinite => Deadline::None,
            TimeControl::Clock { time_ms, increment_ms, moves_to_go } => {
                let moves = moves_to_go.unwrap_or(20);
                let time_soft = time_ms / (moves + 10) + increment_ms / 5;
                let time_hard= time_ms / moves + increment_ms;
                let soft_time = start + std::time::Duration::from_millis(time_soft as u64);
                let hard_time = start + std::time::Duration::from_millis(time_hard as u64);
                Deadline::Time(soft_time, hard_time)
            },
        }
    }
    pub fn check_soft(&self, now: Instant, nodes_searched: usize, depth_searched: usize) -> bool {
        match self {
            Deadline::Depth(d) => depth_searched >= *d,
            Deadline::Nodes(n) => nodes_searched >= *n,
            Deadline::Time(soft, _) => now >= *soft,
            Deadline::None => false,
        }
    }
    pub fn check_hard(&self, now: Instant, nodes_searched: usize, depth_searched: usize) -> bool {
        match self {
            Deadline::Depth(d) => depth_searched >= *d,
            Deadline::Nodes(n) => nodes_searched >= *n,
            Deadline::Time(_, hard) => now >= *hard,
            Deadline::None => false,
        }
    }
}