use crate::SnapBuf;
use std::io::{Read, Seek, SeekFrom, Write};

/// A file-like abstraction around SnapBuf implementing std-io traits.
pub struct SnapBufCursor {
    cursor: usize,
    buf: SnapBuf,
}

impl SnapBufCursor {
    /// Wraps an existing `SnapBuf`.
    ///
    /// Initially, the cursor is at the start of the buffer
    pub fn new(snap_buf: SnapBuf) -> Self {
        SnapBufCursor {
            cursor: 0,
            buf: snap_buf,
        }
    }

    /// Returns the contained `SnapBuf`.
    ///
    /// The cursor position is discarded.
    pub fn into_inner(self) -> SnapBuf {
        self.buf
    }
}

impl Seek for SnapBufCursor {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let new_pos = match pos {
            SeekFrom::Start(x) => {
                self.cursor = x.try_into().unwrap();
                return Ok(x);
            }
            SeekFrom::End(x) => self.buf.len() as i64 + x,
            SeekFrom::Current(x) => self.cursor as i64 + x,
        };
        if new_pos < 0 {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        self.cursor = new_pos.try_into().unwrap();
        Ok(self.cursor as u64)
    }
}

impl Read for SnapBufCursor {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let available = self.buf.read(self.cursor);
        let read_len = buf.len().min(available.len());
        buf[..read_len].copy_from_slice(&available[..read_len]);
        self.cursor += read_len;
        Ok(read_len)
    }
}

impl Write for SnapBufCursor {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buf.write(self.cursor, buf);
        self.cursor += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
