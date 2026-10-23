//! Reading a dictionary file's (uncompressed) bytes: plain files in place, dictzip (`.dz`) by
//! decompressing only the chunks a range touches, plain gzip by decompressing it once into memory.
//!
//! dictzip is gzip with an extra header field ("RA") listing the compressed size of every chunk
//! (usually 58 KB of text each, flushed independently), so any chunk can be inflated on its own.

use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use flate2::read::{GzDecoder, MultiGzDecoder};
use flate2::{Decompress, FlushDecompress};

pub enum Source {
    Plain { file: Mutex<File> },
    DictZip(DictZip),
    Memory(Vec<u8>),
}

pub struct DictZip {
    file: Mutex<File>,
    chunk_len: u64,
    /// compressed start of every chunk, plus the end of the last one
    starts: Vec<u64>,
    /// the chunk inflated last (articles are usually read one after another nearby)
    last: Mutex<Option<(usize, Vec<u8>)>>,
}

impl Source {
    pub fn open(path: &Path) -> io::Result<Source> {
        let mut file = File::open(path)?;
        let mut magic = [0u8; 2];
        let n = file.read(&mut magic)?;
        file.seek(SeekFrom::Start(0))?;
        if n == 2 && magic == [0x1f, 0x8b] {
            if let Some((chunk_len, starts)) = DictZip::parse_header(&mut file)? {
                return Ok(Source::DictZip(DictZip {
                    file: Mutex::new(file),
                    chunk_len,
                    starts,
                    last: Mutex::new(None),
                }));
            }
            let mut data = Vec::new();
            MultiGzDecoder::new(BufReader::new(file)).read_to_end(&mut data)?;
            return Ok(Source::Memory(data));
        }
        Ok(Source::Plain {
            file: Mutex::new(file),
        })
    }

    /// The whole uncompressed content as a stream (for indexing).
    pub fn stream(path: &Path) -> io::Result<Box<dyn Read>> {
        let mut file = File::open(path)?;
        let mut magic = [0u8; 2];
        let n = file.read(&mut magic)?;
        file.seek(SeekFrom::Start(0))?;
        let r = BufReader::with_capacity(1 << 20, file);
        Ok(if n == 2 && magic == [0x1f, 0x8b] {
            Box::new(GzDecoder::new(r))
        } else {
            Box::new(r)
        })
    }

    /// Up to `len` bytes starting at uncompressed `offset` (fewer at the end of the file).
    pub fn read_at(&self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        match self {
            Source::Plain { file } => {
                let mut f = file.lock().unwrap();
                f.seek(SeekFrom::Start(offset))?;
                let mut buf = Vec::with_capacity(len);
                f.by_ref().take(len as u64).read_to_end(&mut buf)?;
                Ok(buf)
            }
            Source::Memory(data) => {
                let s = (offset as usize).min(data.len());
                let e = s.saturating_add(len).min(data.len());
                Ok(data[s..e].to_vec())
            }
            Source::DictZip(dz) => dz.read_at(offset, len),
        }
    }
}

impl DictZip {
    /// Chunk length and chunk starts from the gzip header; `None` when there is no RA field (plain gzip).
    fn parse_header(file: &mut File) -> io::Result<Option<(u64, Vec<u64>)>> {
        let mut r = BufReader::new(&mut *file);
        let mut head = [0u8; 10];
        r.read_exact(&mut head)?;
        let flags = head[3];
        let mut pos = 10u64;
        let mut ra: Option<(u64, Vec<u64>)> = None;
        if flags & 0x04 != 0 {
            let xlen = read_u16(&mut r)? as usize;
            let mut extra = vec![0u8; xlen];
            r.read_exact(&mut extra)?;
            pos += 2 + xlen as u64;
            let mut i = 0;
            while i + 4 <= extra.len() {
                let (si1, si2) = (extra[i], extra[i + 1]);
                let len = u16::from_le_bytes([extra[i + 2], extra[i + 3]]) as usize;
                let data = &extra[i + 4..(i + 4 + len).min(extra.len())];
                if si1 == b'R' && si2 == b'A' && data.len() >= 6 {
                    let chunk_len = u16::from_le_bytes([data[2], data[3]]) as u64;
                    let count = u16::from_le_bytes([data[4], data[5]]) as usize;
                    let sizes = (0..count)
                        .filter_map(|k| data.get(6 + 2 * k..8 + 2 * k))
                        .map(|b| u16::from_le_bytes([b[0], b[1]]) as u64)
                        .collect::<Vec<_>>();
                    ra = Some((chunk_len, sizes));
                }
                i += 4 + len;
            }
        }
        let Some((chunk_len, sizes)) = ra else {
            return Ok(None);
        };
        if chunk_len == 0 || sizes.is_empty() {
            return Ok(None);
        }
        for flag in [0x08u8, 0x10] {
            // file name, comment: zero-terminated
            if flags & flag != 0 {
                let mut b = [0u8; 1];
                loop {
                    r.read_exact(&mut b)?;
                    pos += 1;
                    if b[0] == 0 {
                        break;
                    }
                }
            }
        }
        if flags & 0x02 != 0 {
            pos += 2; // header CRC
        }
        let mut starts = Vec::with_capacity(sizes.len() + 1);
        let mut at = pos;
        for s in sizes {
            starts.push(at);
            at += s;
        }
        starts.push(at);
        Ok(Some((chunk_len, starts)))
    }

    fn chunk(&self, i: usize) -> io::Result<Vec<u8>> {
        {
            let last = self.last.lock().unwrap();
            if let Some((n, data)) = last.as_ref() {
                if *n == i {
                    return Ok(data.clone());
                }
            }
        }
        let (s, e) = (self.starts[i], self.starts[i + 1]);
        let mut comp = vec![0u8; (e - s) as usize];
        {
            let mut f = self.file.lock().unwrap();
            f.seek(SeekFrom::Start(s))?;
            f.read_exact(&mut comp)?;
        }
        let mut out = Vec::with_capacity(self.chunk_len as usize);
        let mut inflate = Decompress::new(false);
        inflate
            .decompress_vec(&comp, &mut out, FlushDecompress::Sync)
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("dictzip chunk {i}: {e}"),
                )
            })?;
        *self.last.lock().unwrap() = Some((i, out.clone()));
        Ok(out)
    }

    fn read_at(&self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        let count = self.starts.len() - 1;
        let mut out = Vec::with_capacity(len);
        let mut i = (offset / self.chunk_len) as usize;
        let mut skip = (offset % self.chunk_len) as usize;
        while out.len() < len && i < count {
            let data = self.chunk(i)?;
            if skip < data.len() {
                let take = (len - out.len()).min(data.len() - skip);
                out.extend_from_slice(&data[skip..skip + take]);
            }
            skip = 0;
            i += 1;
        }
        Ok(out)
    }
}

fn read_u16(r: &mut impl Read) -> io::Result<u16> {
    let mut b = [0u8; 2];
    r.read_exact(&mut b)?;
    Ok(u16::from_le_bytes(b))
}

/// The path without a trailing `.dz` (`x.dsl.dz` → `x.dsl`).
pub fn without_dz(path: &Path) -> PathBuf {
    match path.to_str() {
        Some(s) if s.to_ascii_lowercase().ends_with(".dz") => PathBuf::from(&s[..s.len() - 3]),
        _ => path.to_path_buf(),
    }
}

/// Write `data` as a dictzip file (used by the tests, and handy for making `.dz` test files).
pub fn write_dictzip(path: &Path, data: &[u8], chunk_len: usize) -> io::Result<()> {
    use flate2::{Compress, Compression, FlushCompress};
    use std::io::Write;
    let chunks: Vec<&[u8]> = if data.is_empty() {
        vec![&[][..]]
    } else {
        data.chunks(chunk_len).collect()
    };
    let mut comp = Compress::new(Compression::default(), false);
    let mut bodies = Vec::new();
    for c in &chunks {
        let mut out = Vec::with_capacity(c.len() + 64);
        let before = comp.total_in();
        loop {
            let consumed = (comp.total_in() - before) as usize;
            out.reserve(1024);
            comp.compress_vec(&c[consumed..], &mut out, FlushCompress::Full)
                .map_err(io::Error::other)?;
            if (comp.total_in() - before) as usize == c.len() && out.len() < out.capacity() {
                break;
            }
        }
        bodies.push(out);
    }
    let mut tail = Vec::with_capacity(64);
    while comp
        .compress_vec(&[], &mut tail, FlushCompress::Finish)
        .map_err(io::Error::other)?
        != flate2::Status::StreamEnd
    {
        tail.reserve(64);
    }
    if let Some(last) = bodies.last_mut() {
        last.extend_from_slice(&tail);
    }
    let mut ra = vec![b'R', b'A'];
    let ra_len = 6 + 2 * bodies.len();
    ra.extend_from_slice(&(ra_len as u16).to_le_bytes());
    ra.extend_from_slice(&1u16.to_le_bytes());
    ra.extend_from_slice(&(chunk_len as u16).to_le_bytes());
    ra.extend_from_slice(&(bodies.len() as u16).to_le_bytes());
    for b in &bodies {
        ra.extend_from_slice(&(b.len() as u16).to_le_bytes());
    }
    let mut f = io::BufWriter::new(File::create(path)?);
    f.write_all(&[0x1f, 0x8b, 8, 0x04, 0, 0, 0, 0, 0, 3])?;
    f.write_all(&(ra.len() as u16).to_le_bytes())?;
    f.write_all(&ra)?;
    for b in &bodies {
        f.write_all(b)?;
    }
    let mut crc = flate2::Crc::new();
    crc.update(data);
    f.write_all(&crc.sum().to_le_bytes())?;
    f.write_all(&(data.len() as u32).to_le_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictzip_random_access_and_stream() {
        let dir = tempfile::tempdir().unwrap();
        let data: Vec<u8> = (0..300_000u32)
            .flat_map(|i| format!("line {i}\n").into_bytes())
            .collect();
        let p = dir.path().join("t.dsl.dz");
        write_dictzip(&p, &data, 58315).unwrap();
        let s = Source::open(&p).unwrap();
        assert!(matches!(s, Source::DictZip(_)));
        for (off, len) in [
            (0u64, 10usize),
            (58310, 20),
            (1_000_000, 70_000),
            (data.len() as u64 - 5, 50),
        ] {
            let got = s.read_at(off, len).unwrap();
            let e = (off as usize + len).min(data.len());
            assert_eq!(got, &data[off as usize..e], "at {off}");
        }
        let mut all = Vec::new();
        Source::stream(&p).unwrap().read_to_end(&mut all).unwrap();
        assert_eq!(all, data);
    }
}
