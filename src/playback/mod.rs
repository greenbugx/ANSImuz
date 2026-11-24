pub mod events;
pub mod ipc;
pub mod mpv;
#[cfg(feature = "native-mpv")]
pub mod native_adapter;
#[cfg(feature = "native-mpv")]
pub mod native_backend;
#[cfg(feature = "native-mpv")]
pub mod native_mpv;
pub mod player;
pub mod repeat;

pub use events::PlayerEvent;
pub use mpv::{CliPlayer, PrefetchResult, resolve_stream};
#[cfg(feature = "native-mpv")]
pub use native_adapter::Translator;
#[cfg(feature = "native-mpv")]
pub use native_backend::NativeBackend;
pub use player::AudioPlayer;
pub use repeat::Repeat;
