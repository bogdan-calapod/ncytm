//! Stub module for the playback worker.
//! This will be replaced with YouTube Music playback implementation.

use crate::model::playable::Playable;

/// Commands that can be sent to the player worker.
#[derive(Debug)]
pub(crate) enum WorkerCommand {
    Load(Playable, bool, u32),
    Play,
    Pause,
    Stop,
    Seek(u32),
    SetVolume(u16),
    Preload(Playable),
    Shutdown,
}
