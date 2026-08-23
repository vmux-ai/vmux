use std::io::{BufRead, BufReader, Read};

/// Incremental reader over AXe's `multipart/x-mixed-replace` MJPEG output.
///
/// AXe prefixes the stream with an HTTP status line and then repeats
/// `--<boundary>` / `Content-Type` / `Content-Length` / blank line / payload. The
/// `Content-Length` is what makes framing exact — scanning for JPEG end markers would
/// false-positive on the EXIF thumbnail every frame carries.
pub struct MjpegReader<R: Read> {
    inner: BufReader<R>,
    line: String,
}

impl<R: Read> MjpegReader<R> {
    pub fn new(source: R) -> Self {
        Self {
            inner: BufReader::new(source),
            line: String::new(),
        }
    }

    /// Next JPEG payload, or `None` once the stream ends or goes unparseable.
    pub fn next_frame(&mut self) -> Option<Vec<u8>> {
        let length = self.read_part_length()?;
        let mut payload = vec![0u8; length];
        self.inner.read_exact(&mut payload).ok()?;
        Some(payload)
    }

    /// Consumes headers up to and including the blank line that ends them.
    fn read_part_length(&mut self) -> Option<usize> {
        let mut length = None;
        loop {
            self.line.clear();
            let read = self.inner.read_line(&mut self.line).ok()?;
            if read == 0 {
                return None;
            }
            let trimmed = self.line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                if let Some(length) = length {
                    return Some(length);
                }
                continue;
            }
            let Some((name, value)) = trimmed.split_once(':') else {
                continue;
            };
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse::<usize>().ok();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl MjpegReader<std::io::Cursor<Vec<u8>>> {
        fn over(parts: &[&[u8]]) -> Self {
            let mut buf =
                b"HTTP/1.1 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary=--mjpegstream\r\n\r\n"
                    .to_vec();
            for part in parts {
                buf.extend_from_slice(b"--mjpegstream\r\nContent-Type: image/jpeg\r\n");
                buf.extend_from_slice(format!("Content-Length: {}\r\n\r\n", part.len()).as_bytes());
                buf.extend_from_slice(part);
                buf.extend_from_slice(b"\r\n");
            }
            Self::new(std::io::Cursor::new(buf))
        }
    }

    #[test]
    fn reads_each_payload_exactly() {
        let first = vec![0xABu8; 64];
        let second = vec![0xCDu8; 130];
        let mut reader = MjpegReader::over(&[&first, &second]);

        assert_eq!(reader.next_frame(), Some(first));
        assert_eq!(reader.next_frame(), Some(second));
        assert_eq!(reader.next_frame(), None);
    }

    #[test]
    fn payload_containing_boundary_and_jpeg_end_marker_stays_intact() {
        let mut payload = b"--mjpegstream\r\nContent-Length: 9\r\n\r\n".to_vec();
        payload.extend_from_slice(&[0xFF, 0xD9]);
        payload.extend_from_slice(&[0x42; 32]);
        let mut reader = MjpegReader::over(&[&payload]);

        assert_eq!(reader.next_frame(), Some(payload));
    }

    #[test]
    fn truncated_payload_yields_nothing() {
        let mut buf = b"Content-Length: 100\r\n\r\n".to_vec();
        buf.extend_from_slice(&[0u8; 40]);
        let mut reader = MjpegReader::new(std::io::Cursor::new(buf));

        assert_eq!(reader.next_frame(), None);
    }
}
