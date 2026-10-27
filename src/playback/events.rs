pub enum PlayerEvent {
    Position {
        generation: u64,
        elapsed: f64,
        duration: Option<f64>,
    },
    Failed {
        generation: u64,
        message: String,
    },
}
