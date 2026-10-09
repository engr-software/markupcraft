//! Image files beyond what the `image` crate is built for here: GIF read (the first frame) and
//! write (pages exported as GIF), and multi-page TIFF (every page of a scan set becomes a PDF
//! page). GIF's LZW coding is `weezl`'s; the rest is the GIF89a and TIFF 6.0 file layouts.

use std::io::Cursor;
use std::sync::Arc;

use markupcraft_revu::cos::{Dict, Document as CosDoc, Object, SaveOptions, Stream, write_full};
use markupcraft_revu::pdf::n;

use crate::blank::MAX_SIDE;
use crate::docutil::page_objs;
use crate::{Result, invalid};

/// Largest image side, pixels.
const MAX_PX: u32 = 30_000;
/// Most pages read from one TIFF.
pub const MAX_TIFF_PAGES: usize = 2_000;

/// A decoded picture: RGB over white, its size and its resolution (pixels per inch).
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub w: u32,
    pub h: u32,
    pub rgb: Vec<u8>,
    pub ppi: f64,
}

pub fn is_gif(b: &[u8]) -> bool {
    b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a")
}

pub fn is_tiff(b: &[u8]) -> bool {
    b.starts_with(b"II*\0") || b.starts_with(b"MM\0*") || b.starts_with(b"II+\0") || b.starts_with(b"MM\0+")
}

// ---- GIF ---------------------------------------------------------------------------------------

/// The first frame of a GIF, composited over white.
pub fn gif_decode(b: &[u8]) -> Result<Picture> {
    let bad = || invalid("not a readable GIF");
    if !is_gif(b) {
        return Err(bad());
    }
    let le = |i: usize| -> Result<u16> {
        Ok(u16::from_le_bytes([
            *b.get(i).ok_or_else(bad)?,
            *b.get(i + 1).ok_or_else(bad)?,
        ]))
    };
    let (sw, sh) = (u32::from(le(6)?), u32::from(le(8)?));
    let flags = *b.get(10).ok_or_else(bad)?;
    let mut at = 13usize;
    let mut global: Vec<[u8; 3]> = Vec::new();
    if flags & 0x80 != 0 {
        let n = 1usize << ((flags & 7) + 1);
        for i in 0..n {
            let p = b.get(at + i * 3..at + i * 3 + 3).ok_or_else(bad)?;
            global.push([p[0], p[1], p[2]]);
        }
        at += n * 3;
    }
    let mut transparent: Option<u8> = None;
    for _ in 0..100_000 {
        match *b.get(at).ok_or_else(bad)? {
            0x21 => {
                let label = *b.get(at + 1).ok_or_else(bad)?;
                at += 2;
                if label == 0xF9 && *b.get(at).ok_or_else(bad)? >= 4 {
                    let pf = *b.get(at + 1).ok_or_else(bad)?;
                    if pf & 1 != 0 {
                        transparent = Some(*b.get(at + 4).ok_or_else(bad)?);
                    }
                }
                // skip the sub-blocks
                loop {
                    let len = usize::from(*b.get(at).ok_or_else(bad)?);
                    at += 1 + len;
                    if len == 0 {
                        break;
                    }
                }
            }
            0x2C => {
                let (fx, fy) = (u32::from(le(at + 1)?), u32::from(le(at + 3)?));
                let (fw, fh) = (u32::from(le(at + 5)?), u32::from(le(at + 7)?));
                let lf = *b.get(at + 9).ok_or_else(bad)?;
                at += 10;
                let mut table = global.clone();
                if lf & 0x80 != 0 {
                    let n = 1usize << ((lf & 7) + 1);
                    table.clear();
                    for i in 0..n {
                        let p = b.get(at + i * 3..at + i * 3 + 3).ok_or_else(bad)?;
                        table.push([p[0], p[1], p[2]]);
                    }
                    at += n * 3;
                }
                let interlaced = lf & 0x40 != 0;
                let min = *b.get(at).ok_or_else(bad)?;
                at += 1;
                if !(2..=11).contains(&min) {
                    return Err(bad());
                }
                let mut data = Vec::new();
                loop {
                    let len = usize::from(*b.get(at).ok_or_else(bad)?);
                    data.extend_from_slice(b.get(at + 1..at + 1 + len).ok_or_else(bad)?);
                    at += 1 + len;
                    if len == 0 {
                        break;
                    }
                }
                let (w, h) = (sw.max(fx + fw), sh.max(fy + fh));
                if w == 0 || h == 0 || w > MAX_PX || h > MAX_PX {
                    return Err(invalid("the GIF's size is not usable"));
                }
                let idx = weezl::decode::Decoder::new(weezl::BitOrder::Lsb, min)
                    .decode(&data)
                    .map_err(|e| invalid(format!("GIF data: {e}")))?;
                let mut rgb = vec![255u8; (w as usize) * (h as usize) * 3];
                let rows: Vec<u32> = if interlaced {
                    let mut r = Vec::with_capacity(fh as usize);
                    for (start, step) in [(0, 8), (4, 8), (2, 4), (1, 2)] {
                        let mut y = start;
                        while y < fh {
                            r.push(y);
                            y += step;
                        }
                    }
                    r
                } else {
                    (0..fh).collect()
                };
                for (i, &ix) in idx.iter().enumerate().take((fw as usize) * (fh as usize)) {
                    if Some(ix) == transparent {
                        continue;
                    }
                    let (row, col) = (i / fw as usize, i % fw as usize);
                    let Some(&y) = rows.get(row) else { continue };
                    let (x, y) = (fx as usize + col, (fy + y) as usize);
                    let Some(c) = table.get(usize::from(ix)) else { continue };
                    let o = (y * w as usize + x) * 3;
                    if let Some(px) = rgb.get_mut(o..o + 3) {
                        px.copy_from_slice(c);
                    }
                }
                return Ok(Picture { w, h, rgb, ppi: 72.0 });
            }
            _ => return Err(bad()),
        }
    }
    Err(bad())
}

/// The 256 colours GIF export uses: a 6 x 7 x 6 colour cube and 4 more greys.
fn palette() -> Vec<[u8; 3]> {
    let mut p = Vec::with_capacity(256);
    for r in 0..6u32 {
        for g in 0..7u32 {
            for b in 0..6u32 {
                p.push([(r * 51) as u8, (g * 255 / 6) as u8, (b * 51) as u8]);
            }
        }
    }
    for v in [64u8, 128, 192, 224] {
        p.push([v, v, v]);
    }
    p
}

fn nearest(pal: &[[u8; 3]], c: [u8; 3]) -> u8 {
    let r = (u32::from(c[0]) * 5 + 127) / 255;
    let g = (u32::from(c[1]) * 6 + 127) / 255;
    let b = (u32::from(c[2]) * 5 + 127) / 255;
    let cube = (r * 42 + g * 6 + b) as usize;
    let d = |p: &[u8; 3]| (0..3).map(|i| (i32::from(p[i]) - i32::from(c[i])).pow(2)).sum::<i32>();
    let mut best = (cube, pal.get(cube).map_or(i32::MAX, d));
    for (i, p) in pal.iter().enumerate().skip(252) {
        let v = d(p);
        if v < best.1 {
            best = (i, v);
        }
    }
    best.0.min(255) as u8
}

/// Encode an RGB image as a GIF89a (256 fixed colours).
pub fn gif_encode(w: usize, h: usize, rgb: &[u8]) -> Result<Vec<u8>> {
    if w == 0 || h == 0 || w > 65_535 || h > 65_535 || rgb.len() < w * h * 3 {
        return Err(invalid("the image is too large for a GIF (65,535 pixels a side)"));
    }
    let pal = palette();
    let idx: Vec<u8> = rgb
        .as_chunks::<3>()
        .0
        .iter()
        .take(w * h)
        .map(|p| nearest(&pal, *p))
        .collect();
    let lzw = weezl::encode::Encoder::new(weezl::BitOrder::Lsb, 8)
        .encode(&idx)
        .map_err(|e| invalid(format!("GIF encoding: {e}")))?;
    let mut out = Vec::with_capacity(lzw.len() + 1024);
    out.extend_from_slice(b"GIF89a");
    out.extend_from_slice(&(w as u16).to_le_bytes());
    out.extend_from_slice(&(h as u16).to_le_bytes());
    out.extend_from_slice(&[0xF7, 0, 0]);
    for c in &pal {
        out.extend_from_slice(c);
    }
    out.push(0x2C);
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&(w as u16).to_le_bytes());
    out.extend_from_slice(&(h as u16).to_le_bytes());
    out.push(0);
    out.push(8);
    for chunk in lzw.chunks(255) {
        out.push(chunk.len() as u8);
        out.extend_from_slice(chunk);
    }
    out.push(0);
    out.push(0x3B);
    Ok(out)
}

// ---- TIFF --------------------------------------------------------------------------------------

/// The IFD offsets of a TIFF (each a page), and whether it is BigTIFF.
fn tiff_ifds(b: &[u8]) -> Option<(Vec<u64>, bool, bool)> {
    let le = b.starts_with(b"II");
    let big = b.get(2..4)? == if le { [43, 0] } else { [0, 43] };
    let u16_at = |i: usize| -> Option<u16> {
        let s: [u8; 2] = b.get(i..i + 2)?.try_into().ok()?;
        Some(if le {
            u16::from_le_bytes(s)
        } else {
            u16::from_be_bytes(s)
        })
    };
    let u32_at = |i: usize| -> Option<u32> {
        let s: [u8; 4] = b.get(i..i + 4)?.try_into().ok()?;
        Some(if le {
            u32::from_le_bytes(s)
        } else {
            u32::from_be_bytes(s)
        })
    };
    let u64_at = |i: usize| -> Option<u64> {
        let s: [u8; 8] = b.get(i..i + 8)?.try_into().ok()?;
        Some(if le {
            u64::from_le_bytes(s)
        } else {
            u64::from_be_bytes(s)
        })
    };
    let mut next = if big { u64_at(8)? } else { u64::from(u32_at(4)?) };
    let mut out = Vec::new();
    while next != 0 && out.len() < MAX_TIFF_PAGES {
        if out.contains(&next) {
            break; // a loop
        }
        out.push(next);
        let at = usize::try_from(next).ok()?;
        next = if big {
            let n = usize::try_from(u64_at(at)?).ok()?;
            u64_at(at.checked_add(8)?.checked_add(n.checked_mul(20)?)?)?
        } else {
            let n = usize::from(u16_at(at)?);
            u64::from(u32_at(at + 2 + n * 12)?)
        };
    }
    Some((out, le, big))
}

/// Resolution of a classic TIFF IFD (`XResolution`, `ResolutionUnit`), pixels per inch.
fn tiff_ppi(b: &[u8], ifd: u64, le: bool) -> Option<f64> {
    let at = usize::try_from(ifd).ok()?;
    let u16_at = |i: usize| -> Option<u16> {
        let s: [u8; 2] = b.get(i..i + 2)?.try_into().ok()?;
        Some(if le {
            u16::from_le_bytes(s)
        } else {
            u16::from_be_bytes(s)
        })
    };
    let u32_at = |i: usize| -> Option<u32> {
        let s: [u8; 4] = b.get(i..i + 4)?.try_into().ok()?;
        Some(if le {
            u32::from_le_bytes(s)
        } else {
            u32::from_be_bytes(s)
        })
    };
    let n = usize::from(u16_at(at)?);
    let (mut x, mut unit) = (None, 2u16);
    for i in 0..n.min(500) {
        let e = at + 2 + i * 12;
        match u16_at(e)? {
            282 => {
                let off = u32_at(e + 8)? as usize;
                let (num, den) = (u32_at(off)?, u32_at(off + 4)?);
                if den != 0 {
                    x = Some(f64::from(num) / f64::from(den));
                }
            }
            296 => unit = u16_at(e + 8)?,
            _ => {}
        }
    }
    let v = match unit {
        3 => x? * 2.54,
        2 => x?,
        _ => return None,
    };
    (v.is_finite() && (10.0..=10_000.0).contains(&v)).then_some(v)
}

fn decode_with_image(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>)> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| invalid(format!("not a readable image: {e}")))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_PX);
    limits.max_image_height = Some(MAX_PX);
    limits.max_alloc = Some(1 << 30);
    reader.limits(limits);
    let img = reader
        .decode()
        .map_err(|e| invalid(format!("not a readable image: {e}")))?
        .to_rgba8();
    let (w, h) = img.dimensions();
    let mut rgb = Vec::with_capacity((w as usize) * (h as usize) * 3);
    for p in img.pixels() {
        let [r, g, b, a] = p.0;
        let over = |c: u8| ((u16::from(c) * u16::from(a) + 255 * (255 - u16::from(a))) / 255) as u8;
        rgb.extend_from_slice(&[over(r), over(g), over(b)]);
    }
    Ok((w, h, rgb))
}

/// Every page of a TIFF (a copy whose header points at each page's directory is decoded in
/// turn, so every compression and colour type the `image` crate reads works).
pub fn tiff_pages(b: &[u8]) -> Result<Vec<Picture>> {
    let (ifds, le, big) = tiff_ifds(b).ok_or_else(|| invalid("not a readable TIFF"))?;
    if ifds.is_empty() {
        return Err(invalid("the TIFF has no pages"));
    }
    let mut out = Vec::new();
    for ifd in ifds {
        let mut copy = b.to_vec();
        if big {
            let v = if le { ifd.to_le_bytes() } else { ifd.to_be_bytes() };
            if let Some(s) = copy.get_mut(8..16) {
                s.copy_from_slice(&v);
            }
        } else {
            let o = u32::try_from(ifd).map_err(|_| invalid("TIFF offset"))?;
            let v = if le { o.to_le_bytes() } else { o.to_be_bytes() };
            if let Some(s) = copy.get_mut(4..8) {
                s.copy_from_slice(&v);
            }
        }
        let (w, h, rgb) = decode_with_image(&copy)?;
        let ppi = if big { None } else { tiff_ppi(b, ifd, le) }.unwrap_or(72.0);
        out.push(Picture { w, h, rgb, ppi });
    }
    Ok(out)
}

/// The pictures in an image file: every page of a TIFF, the first frame of a GIF, else the one
/// image (PNG, JPEG, BMP).
pub fn pictures(bytes: &[u8]) -> Result<Vec<Picture>> {
    if is_gif(bytes) {
        return Ok(vec![gif_decode(bytes)?]);
    }
    if is_tiff(bytes) {
        return tiff_pages(bytes);
    }
    let (w, h, rgb) = decode_with_image(bytes)?;
    let ppi = crate::docfile::image_ppi(bytes).unwrap_or(72.0);
    Ok(vec![Picture { w, h, rgb, ppi }])
}

/// A PDF with one page per picture, each at its resolution.
pub fn pictures_pdf(pics: &[Picture]) -> Result<Vec<u8>> {
    if pics.is_empty() {
        return Err(invalid("no images"));
    }
    let sizes: Vec<(f64, f64)> = pics
        .iter()
        .map(|p| {
            let k = 72.0 / p.ppi;
            (
                (f64::from(p.w) * k).clamp(3.0, MAX_SIDE),
                (f64::from(p.h) * k).clamp(3.0, MAX_SIDE),
            )
        })
        .collect();
    let mut cos = CosDoc::open(Arc::new(crate::blank::pdf_bytes(&sizes)?))?;
    let pages = page_objs(&cos)?;
    for ((page, p), (pw, ph)) in pages.iter().zip(pics).zip(&sizes) {
        let mut d = Dict::new();
        d.set(b"Type".to_vec(), n("XObject"));
        d.set(b"Subtype".to_vec(), n("Image"));
        d.set(b"Width".to_vec(), Object::Int(i64::from(p.w)));
        d.set(b"Height".to_vec(), Object::Int(i64::from(p.h)));
        d.set(b"ColorSpace".to_vec(), n("DeviceRGB"));
        d.set(b"BitsPerComponent".to_vec(), Object::Int(8));
        let xo = cos.add(Object::Stream(Stream::flate(d, &p.rgb)));
        let content = format!("q {pw:.4} 0 0 {ph:.4} 0 0 cm /Im0 Do Q\n");
        let cs = cos.add(Object::Stream(Stream::flate(Dict::new(), content.as_bytes())));
        let mut xobjs = Dict::new();
        xobjs.set(b"Im0".to_vec(), Object::Ref(xo));
        let mut res = Dict::new();
        res.set(b"XObject".to_vec(), Object::Dict(xobjs));
        cos.update_dict(*page, |pd| {
            pd.set(b"Resources".to_vec(), Object::Dict(res));
            pd.set(b"Contents".to_vec(), Object::Ref(cs));
        })?;
    }
    Ok(write_full(&cos, &SaveOptions::default())?)
}

/// An image file as a PDF: every TIFF page, a GIF's first frame, or the one image.
pub fn image_bytes_pdf(bytes: &[u8]) -> Result<Vec<u8>> {
    pictures_pdf(&pictures(bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiff_of(colors: &[[u8; 3]]) -> Vec<u8> {
        // a multi-page TIFF written with the tiff crate
        let mut out = Cursor::new(Vec::new());
        {
            let mut enc = tiff::encoder::TiffEncoder::new(&mut out).unwrap();
            for c in colors {
                let px: Vec<u8> = (0..20 * 10).flat_map(|_| c.to_vec()).collect();
                enc.write_image::<tiff::encoder::colortype::RGB8>(20, 10, &px).unwrap();
            }
        }
        out.into_inner()
    }

    #[test]
    fn gif_round_trip_and_multi_page_tiff() {
        let (w, h) = (17usize, 9usize);
        let rgb: Vec<u8> = (0..w * h)
            .flat_map(|i| if i % 2 == 0 { [255, 0, 0] } else { [255, 255, 255] })
            .collect();
        let gif = gif_encode(w, h, &rgb).unwrap();
        let back = gif_decode(&gif).unwrap();
        assert_eq!((back.w, back.h), (17, 9));
        assert_eq!(&back.rgb[..6], &[255, 0, 0, 255, 255, 255]);
        assert!(gif_decode(b"GIF89a\x01").is_err());
        let pdf = image_bytes_pdf(&gif).unwrap();
        let s = crate::Session::from_bytes(pdf, "g.pdf").unwrap();
        assert_eq!(s.page_count(), 1);

        let t = tiff_of(&[[255, 0, 0], [0, 0, 255], [0, 255, 0]]);
        let pages = tiff_pages(&t).unwrap();
        assert_eq!(pages.len(), 3);
        assert_eq!(&pages[1].rgb[..3], &[0, 0, 255]);
        let s = crate::Session::from_bytes(image_bytes_pdf(&t).unwrap(), "t.pdf").unwrap();
        assert_eq!(s.page_count(), 3);
        assert!(tiff_pages(b"II*\0\xff\xff\xff\xff").is_err());
    }
}
