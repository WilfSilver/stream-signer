use std::{ops::Range, time::Duration};

pub const MAX_CHUNK_LENGTH: Duration = Duration::from_secs(10);
pub const MIN_CHUNK_LENGTH: Duration = Duration::from_millis(50);
pub const CHUNK_LENGTH_RANGE: Range<Duration> = MIN_CHUNK_LENGTH..MAX_CHUNK_LENGTH;
