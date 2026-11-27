//! The length-capped line codec of stdout (SPEC AG-13).

use tokio_util::bytes::BytesMut;
use tokio_util::codec::{Decoder, LinesCodec, LinesCodecError};

/// A line of stdout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// A line, without its line break.
    Text(String),
    /// A line longer than the cap. Its bytes were discarded through the next
    /// line break, and reading goes on after it.
    TooLong,
    /// A line that is no UTF-8. Its bytes were discarded, and reading goes on
    /// after it.
    NotUtf8,
}

/// `LinesCodec` with the cap, whose two errors of a line are lines here: the
/// stream that reads it does not end at the first of them, as it would end
/// at an error of the decoder.
#[derive(Debug)]
pub(crate) struct LineCodec(LinesCodec);

impl LineCodec {
    pub(crate) fn new(max_length: usize) -> Self {
        Self(LinesCodec::new_with_max_length(max_length))
    }
}

/// The line of what `LinesCodec` decoded.
fn line(decoded: Result<Option<String>, LinesCodecError>) -> Option<Line> {
    match decoded {
        Ok(line) => line.map(Line::Text),
        Err(LinesCodecError::MaxLineLengthExceeded) => Some(Line::TooLong),
        // A decoder does no I/O: its one I/O error is a line that is no
        // UTF-8, which it has consumed already.
        Err(LinesCodecError::Io(_)) => Some(Line::NotUtf8),
    }
}

impl Decoder for LineCodec {
    type Item = Line;
    type Error = std::io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Line>, std::io::Error> {
        Ok(line(self.0.decode(buf)))
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Line>, std::io::Error> {
        Ok(line(self.0.decode_eof(buf)))
    }
}
