#[derive(Clone, Debug, PartialEq)]
pub enum PlayerEvent {
    Position {
        generation: u64,
        elapsed: f64,
        duration: Option<f64>,
    },
    Ended {
        generation: u64,
    },
    Failed {
        generation: u64,
        message: String,
    },
}
