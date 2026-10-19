//! OS lain (mis. Linux untuk development): tidak ada sumber now-playing.

use mini_lyrics_core::PlayerState;

use super::NowPlayingSource;

pub struct Source;

impl Source {
    pub fn new() -> Result<Self, String> {
        Ok(Self)
    }
}

impl NowPlayingSource for Source {
    fn poll(&mut self) -> Result<Option<PlayerState>, String> {
        Ok(None)
    }
}
