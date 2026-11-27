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
    /// Bytes of stdout as they were read, of a command whose stdout is raw
    /// ([`crate::Command::raw_stdout`]): no line, and no cap.
    Bytes(Vec<u8>),
}

/// `LinesCodec` with the cap, whose two errors of a line are lines here: the
/// stream that reads it does not end at the first of them, as it would end
/// at an error of the decoder. A raw codec passes on what it is given.
#[derive(Debug)]
pub(crate) enum LineCodec {
    Lines(LinesCodec),
    Raw,
}

impl LineCodec {
    pub(crate) fn new(max_length: usize) -> Self {
        Self::Lines(LinesCodec::new_with_max_length(max_length))
    }

    /// What a raw codec passes on: every byte it holds, where it holds one.
    fn raw(buf: &mut BytesMut) -> Option<Line> {
        if buf.is_empty() {
            None
        } else {
            Some(Line::Bytes(buf.split().to_vec()))
        }
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
        Ok(match self {
            Self::Lines(codec) => line(codec.decode(buf)),
            Self::Raw => Self::raw(buf),
        })
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Line>, std::io::Error> {
        Ok(match self {
            Self::Lines(codec) => line(codec.decode_eof(buf)),
            Self::Raw => Self::raw(buf),
        })
    }
}
