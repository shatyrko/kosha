//! Text encoding of a DSL file and reading it line by line with byte positions.
//!
//! Lingvo writes DSL as UTF-16 little-endian with a byte-order mark; UTF-8 (with or without a mark)
//! and UTF-16 big-endian are found too, and very old files are in a Windows code page named by the
//! `#SOURCE_CODE_PAGE` header. Positions are byte offsets in the uncompressed file, so an article
//! can be read back later with one `read_at`.

use std::io::{self, Read};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf16Le,
    Utf16Be,
    Utf8,
    Legacy(&'static encoding_rs::Encoding),
}

impl Encoding {
    /// From the first bytes of the file: (encoding, length of the byte-order mark).
    pub fn sniff(head: &[u8]) -> (Encoding, usize) {
        match head {
            [0xFF, 0xFE, ..] => (Encoding::Utf16Le, 2),
            [0xFE, 0xFF, ..] => (Encoding::Utf16Be, 2),
            [0xEF, 0xBB, 0xBF, ..] => (Encoding::Utf8, 3),
            [a, 0, ..] if *a != 0 => (Encoding::Utf16Le, 0),
            [0, b, ..] if *b != 0 => (Encoding::Utf16Be, 0),
            _ if std::str::from_utf8(trim_partial_utf8(head)).is_ok() => (Encoding::Utf8, 0),
            _ => (Encoding::Legacy(encoding_rs::WINDOWS_1252), 0),
        }
    }

    pub fn unit(self) -> usize {
        match self {
            Encoding::Utf16Le | Encoding::Utf16Be => 2,
            _ => 1,
        }
    }

    /// Windows code page names used in `#SOURCE_CODE_PAGE` ("Cyrillic", "Latin", ...).
    pub fn from_code_page(name: &str) -> Option<Encoding> {
        let lower = name.trim().to_ascii_lowercase();
        let label = match lower.as_str() {
            "latin" | "westerneuropean" => "windows-1252",
            "easteuropean" | "centraleuropean" => "windows-1250",
            "cyrillic" => "windows-1251",
            "greek" => "windows-1253",
            "turkish" => "windows-1254",
            "hebrew" => "windows-1255",
            "arabic" => "windows-1256",
            "baltic" => "windows-1257",
            "vietnamese" => "windows-1258",
            "thai" => "windows-874",
            "japanese" => "shift_jis",
            "chinesesimp" | "chinese" => "gbk",
            "chinesetrad" => "big5",
            "korean" => "euc-kr",
            other => other,
        };
        encoding_rs::Encoding::for_label(label.as_bytes()).map(Encoding::Legacy)
    }

    pub fn decode(self, bytes: &[u8]) -> String {
        let mut s = match self {
            Encoding::Utf16Le => encoding_rs::UTF_16LE
                .decode_without_bom_handling(bytes)
                .0
                .into_owned(),
            Encoding::Utf16Be => encoding_rs::UTF_16BE
                .decode_without_bom_handling(bytes)
                .0
                .into_owned(),
            Encoding::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
            Encoding::Legacy(e) => e.decode_without_bom_handling(bytes).0.into_owned(),
        };
        if s.ends_with('\r') {
            s.pop();
        }
        s
    }

    fn is_newline(self, b: &[u8]) -> bool {
        match self {
            Encoding::Utf16Le => b[0] == b'\n' && b[1] == 0,
            Encoding::Utf16Be => b[0] == 0 && b[1] == b'\n',
            _ => b[0] == b'\n',
        }
    }
}

fn trim_partial_utf8(b: &[u8]) -> &[u8] {
    // the sample may end in the middle of a character
    let mut end = b.len();
    while end > 0 && end + 4 > b.len() && (b[end - 1] & 0xC0) == 0x80 {
        end -= 1;
    }
    if end > 0 && end + 4 > b.len() && b[end - 1] >= 0xC0 {
        end -= 1;
    }
    &b[..end]
}

/// Lines of a stream as (byte offset of the line, raw bytes without the newline).
pub struct Lines<R: Read> {
    reader: R,
    enc: Encoding,
    buf: Vec<u8>,
    start: usize,
    /// file offset of buf[0]
    base: u64,
    eof: bool,
}

impl<R: Read> Lines<R> {
    /// `skip` bytes (the byte-order mark) are consumed first and counted in the offsets.
    pub fn new(mut reader: R, enc: Encoding, skip: usize) -> io::Result<Self> {
        let mut bom = vec![0u8; skip];
        reader.read_exact(&mut bom)?;
        Ok(Lines {
            reader,
            enc,
            buf: Vec::with_capacity(1 << 20),
            start: 0,
            base: skip as u64,
            eof: false,
        })
    }

    pub fn next_line(&mut self) -> io::Result<Option<(u64, Vec<u8>)>> {
        let unit = self.enc.unit();
        let mut scan = self.start;
        loop {
            while scan + unit <= self.buf.len() {
                if self.enc.is_newline(&self.buf[scan..scan + unit]) {
                    let line = self.buf[self.start..scan].to_vec();
                    let off = self.base + self.start as u64;
                    self.start = scan + unit;
                    return Ok(Some((off, line)));
                }
                scan += unit;
            }
            if self.eof {
                if self.start < self.buf.len() {
                    let line = self.buf[self.start..].to_vec();
                    let off = self.base + self.start as u64;
                    self.start = self.buf.len();
                    return Ok(Some((off, line)));
                }
                return Ok(None);
            }
            // keep the unfinished line, read more
            self.buf.drain(..self.start);
            self.base += self.start as u64;
            scan -= self.start;
            self.start = 0;
            let old = self.buf.len();
            self.buf.resize(old + (1 << 20), 0);
            let n = read_full(&mut self.reader, &mut self.buf[old..])?;
            self.buf.truncate(old + n);
            if n == 0 {
                self.eof = true;
            }
        }
    }

    /// Offset just past everything read so far (the file length once `next_line` returned `None`).
    pub fn position(&self) -> u64 {
        self.base + self.start as u64
    }
}

fn read_full(r: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(n)
}

pub fn encode_utf16le_with_bom(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    for u in text.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_lines_with_offsets() {
        let bytes = encode_utf16le_with_bom("ab\r\nшум\n\nx");
        let (enc, bom) = Encoding::sniff(&bytes);
        assert_eq!((enc, bom), (Encoding::Utf16Le, 2));
        let mut l = Lines::new(&bytes[..], enc, bom).unwrap();
        let mut got = vec![];
        while let Some((off, line)) = l.next_line().unwrap() {
            got.push((off, enc.decode(&line)));
        }
        assert_eq!(
            got,
            vec![
                (2, "ab".into()),
                (10, "шум".into()),
                (18, "".into()),
                (20, "x".into())
            ]
        );
        assert_eq!(l.position(), bytes.len() as u64);
    }

    #[test]
    fn sniffing() {
        assert_eq!(Encoding::sniff(b"#NAME \"x\"").0, Encoding::Utf8);
        assert_eq!(
            Encoding::sniff(&[0xEF, 0xBB, 0xBF, b'#']),
            (Encoding::Utf8, 3)
        );
        assert_eq!(
            Encoding::from_code_page("Cyrillic"),
            Some(Encoding::Legacy(encoding_rs::WINDOWS_1251))
        );
    }
}
