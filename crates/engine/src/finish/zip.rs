//! A small zip reader (stored and deflated entries, no zip64, no encryption) for the Office
//! files MarkupCraft reads and edits: Word text, Excel sheets and workbooks Quantity Link writes
//! into. Writing goes through `pdfcraft_export::Zip`.

use std::io::Read;

use crate::{Result, invalid};

/// Most entries read from one archive.
const MAX_ENTRIES: usize = 20_000;
/// Largest entry inflated, bytes.
const MAX_ENTRY: usize = 256 << 20;
/// Largest archive read, bytes.
pub const MAX_ARCHIVE: u64 = 512 << 20;

/// One file of an archive, inflated.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub name: String,
    pub data: Vec<u8>,
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at.checked_add(2)?)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at.checked_add(4)?)?.try_into().ok()?))
}

/// Every entry of the archive in `bytes`, in central-directory order.
pub fn read(bytes: &[u8]) -> Result<Vec<Entry>> {
    let bad = || invalid("not a readable zip archive (Office files are zip archives)");
    // End of central directory: the last "PK\x05\x06" within the trailing 64 KiB.
    let tail = bytes.len().saturating_sub(65_557);
    let eocd = (tail..bytes.len().saturating_sub(21))
        .rev()
        .find(|&i| bytes.get(i..i + 4) == Some(b"PK\x05\x06"))
        .ok_or_else(bad)?;
    let count = usize::from(u16_at(bytes, eocd + 10).ok_or_else(bad)?);
    let mut at = u32_at(bytes, eocd + 16).ok_or_else(bad)? as usize;
    if count > MAX_ENTRIES {
        return Err(invalid("the archive has too many entries"));
    }
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if bytes.get(at..at + 4) != Some(b"PK\x01\x02") {
            return Err(bad());
        }
        let flags = u16_at(bytes, at + 8).ok_or_else(bad)?;
        let method = u16_at(bytes, at + 10).ok_or_else(bad)?;
        let csize = u32_at(bytes, at + 20).ok_or_else(bad)? as usize;
        let usize_ = u32_at(bytes, at + 24).ok_or_else(bad)? as usize;
        let nlen = usize::from(u16_at(bytes, at + 28).ok_or_else(bad)?);
        let xlen = usize::from(u16_at(bytes, at + 30).ok_or_else(bad)?);
        let clen = usize::from(u16_at(bytes, at + 32).ok_or_else(bad)?);
        let local = u32_at(bytes, at + 42).ok_or_else(bad)? as usize;
        let name = String::from_utf8_lossy(bytes.get(at + 46..at + 46 + nlen).ok_or_else(bad)?).into_owned();
        at = at.checked_add(46 + nlen + xlen + clen).ok_or_else(bad)?;
        if flags & 1 != 0 {
            return Err(invalid(format!("{name} is encrypted")));
        }
        if bytes.get(local..local + 4) != Some(b"PK\x03\x04") {
            return Err(bad());
        }
        let lnlen = usize::from(u16_at(bytes, local + 26).ok_or_else(bad)?);
        let lxlen = usize::from(u16_at(bytes, local + 28).ok_or_else(bad)?);
        let start = local + 30 + lnlen + lxlen;
        let raw = bytes
            .get(start..start.checked_add(csize).ok_or_else(bad)?)
            .ok_or_else(bad)?;
        if usize_ > MAX_ENTRY {
            return Err(invalid(format!("{name} is too large")));
        }
        let data = match method {
            0 => raw.to_vec(),
            8 => {
                let mut d = Vec::with_capacity(usize_.min(MAX_ENTRY));
                flate2::read::DeflateDecoder::new(raw)
                    .take(MAX_ENTRY as u64 + 1)
                    .read_to_end(&mut d)
                    .map_err(|e| invalid(format!("{name}: {e}")))?;
                if d.len() > MAX_ENTRY {
                    return Err(invalid(format!("{name} is too large")));
                }
                d
            }
            m => return Err(invalid(format!("{name}: compression method {m} is not supported"))),
        };
        out.push(Entry { name, data });
    }
    Ok(out)
}

/// The entry called `name` (case-insensitive, `/` or `\`).
pub fn find<'a>(entries: &'a [Entry], name: &str) -> Option<&'a Entry> {
    let want = name.replace('\\', "/").trim_start_matches('/').to_ascii_lowercase();
    entries
        .iter()
        .find(|e| e.name.replace('\\', "/").to_ascii_lowercase() == want)
}

/// Write `entries` as a zip archive (images and other compressed media stored, the rest
/// deflated).
pub fn write(entries: &[Entry]) -> Vec<u8> {
    let mut z = pdfcraft_export::Zip::default();
    for e in entries {
        let lower = e.name.to_ascii_lowercase();
        let stored = [".png", ".jpg", ".jpeg", ".gif", ".zip"]
            .iter()
            .any(|x| lower.ends_with(x));
        z.add(&e.name, &e.data, !stored);
    }
    z.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_bad_input() {
        let entries = vec![
            Entry {
                name: "a.xml".into(),
                data: b"<a>hello hello hello</a>".to_vec(),
            },
            Entry {
                name: "media/p.png".into(),
                data: vec![1, 2, 3],
            },
        ];
        let back = read(&write(&entries)).unwrap();
        assert_eq!(back, entries);
        assert!(find(&back, "/A.XML").is_some());
        assert!(read(b"PK nothing").is_err());
        assert!(read(&[]).is_err());
    }
}
