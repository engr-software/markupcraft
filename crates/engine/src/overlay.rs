//! Overlay Pages: stack pages of two or more PDFs in a new PDF, each recoloured to one colour
//! and kept as its own toggleable layer (a PDF optional content group).
//!
//! Each source page becomes a form XObject (vector content, nothing rasterized). A layer draws
//! that page on white inside an isolated transparency group and lays its colour over it with
//! blend mode Lighten, so ink takes the layer colour and paper stays white. The layers are
//! then stacked with blend mode Multiply: linework every layer has goes dark, linework only
//! one layer has keeps that layer's colour.
//!
//! Alignment maps each layer's page onto the first layer's page: as positioned (page align),
//! stretched to the first page's bounds, or by two matching points (scale, rotation and
//! offset from the pairs).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use markupcraft_geom::{Point, Rect};
use markupcraft_model::Color;
use markupcraft_revu::cos::{Dict, Document as CosDoc, ObjRef, Object, PdfString, SaveOptions, Stream, write_full};

use crate::{EngineError, Result, invalid};

/// Most layers in one overlay.
pub const MAX_LAYERS: usize = 16;
/// Most output pages.
pub const MAX_PAGES: usize = 2_000;
const MAX_OBJECTS: usize = 2_000_000;
const MAX_DEPTH: usize = 64;
const MAX_CONTENT: usize = 256 << 20;

/// How a layer is placed on the first layer's page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OverlayAlign {
    /// As positioned: the crop boxes' lower-left corners coincide.
    Page,
    /// Stretched to the first layer's crop box.
    Bounds,
    /// `from[i]` on this layer's page lands on `to[i]` on the first layer's page (uniform scale,
    /// rotation and offset).
    Points { from: [Point; 2], to: [Point; 2] },
}

/// One layer: a PDF, the pages it contributes, its look.
#[derive(Debug, Clone)]
pub struct OverlayLayer {
    pub bytes: Arc<Vec<u8>>,
    /// Pages (0-based) for output pages 1, 2, ...; empty: every page in order.
    pub pages: Vec<usize>,
    pub color: Color,
    /// 0 to 1.
    pub opacity: f64,
    /// Layer name in the result ("" = "Layer n").
    pub name: String,
    pub align: OverlayAlign,
}

/// The default colours: red, blue, green, magenta, orange, cyan.
pub fn default_color(i: usize) -> Color {
    const C: [(f64, f64, f64); 6] = [
        (1.0, 0.0, 0.0),
        (0.0, 0.3, 1.0),
        (0.0, 0.65, 0.0),
        (0.85, 0.0, 0.85),
        (1.0, 0.5, 0.0),
        (0.0, 0.7, 0.8),
    ];
    let (r, g, b) = C.get(i % C.len()).copied().unwrap_or((1.0, 0.0, 0.0));
    Color::rgb(r, g, b)
}

/// What an overlay wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayReport {
    pub pages: usize,
    pub layers: Vec<String>,
}

/// An affine map `[a b c d e f]` as in a PDF `cm`.
type Matrix = [f64; 6];

fn then(m: Matrix, n: Matrix) -> Matrix {
    [
        n[0] * m[0] + n[2] * m[1],
        n[1] * m[0] + n[3] * m[1],
        n[0] * m[2] + n[2] * m[3],
        n[1] * m[2] + n[3] * m[3],
        n[0] * m[4] + n[2] * m[5] + n[4],
        n[1] * m[4] + n[3] * m[5] + n[5],
    ]
}

/// The similarity transform sending `from[0], from[1]` to `to[0], to[1]`.
pub fn two_point_matrix(from: [Point; 2], to: [Point; 2]) -> Option<[f64; 6]> {
    let (vx, vy) = (from[1].x - from[0].x, from[1].y - from[0].y);
    let (wx, wy) = (to[1].x - to[0].x, to[1].y - to[0].y);
    let len = vx.hypot(vy);
    if !(len.is_finite() && len > 1e-6) || !wx.hypot(wy).is_finite() {
        return None;
    }
    // (wx + i wy) / (vx + i vy) as a complex number: scale and rotation together.
    let d = vx * vx + vy * vy;
    let (c, s) = ((wx * vx + wy * vy) / d, (wy * vx - wx * vy) / d);
    let m = [c, s, -s, c, 0.0, 0.0];
    let e = to[0].x - (m[0] * from[0].x + m[2] * from[0].y);
    let f = to[0].y - (m[1] * from[0].x + m[3] * from[0].y);
    let out = [m[0], m[1], m[2], m[3], e, f];
    out.iter().all(|v| v.is_finite()).then_some(out)
}

/// A page with the attributes it inherits.
struct SrcPage {
    dict: Dict,
    resources: Option<Object>,
    box_: Rect,
}

fn num_rect(cos: &CosDoc, o: Option<&Object>) -> Option<Rect> {
    let o = cos.resolve(o?);
    let a = o.as_array()?;
    let v: Vec<f64> = a.iter().filter_map(|x| cos.resolve(x).as_f64()).collect();
    match v.as_slice() {
        [x0, y0, x1, y1] if v.iter().all(|x| x.is_finite()) => {
            Some(Rect::new(x0.min(*x1), y0.min(*y1), x0.max(*x1), y0.max(*y1)))
        }
        _ => None,
    }
}

fn source_pages(cos: &CosDoc) -> Result<Vec<SrcPage>> {
    let root = cos.root().ok_or_else(|| invalid("the PDF has no catalog"))?;
    let pages = cos
        .dict(&Object::Ref(root))
        .and_then(|d| d.reference(b"Pages"))
        .ok_or_else(|| invalid("the PDF has no page tree"))?;
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    // (node, inherited Resources, MediaBox, CropBox, depth)
    type Node = (ObjRef, Option<Object>, Option<Object>, Option<Object>, usize);
    let mut stack: Vec<Node> = vec![(pages, None, None, None, 0)];
    while let Some((node, res, media, crop, depth)) = stack.pop() {
        if depth > MAX_DEPTH || !seen.insert(node) || out.len() >= 100_000 {
            continue;
        }
        let Some(d) = cos.dict(&Object::Ref(node)) else {
            continue;
        };
        let res = d.get(b"Resources").cloned().or(res);
        let media = d.get(b"MediaBox").cloned().or(media);
        let crop = d.get(b"CropBox").cloned().or(crop);
        if let Some(kids) = d.get(b"Kids").map(|k| cos.resolve(k))
            && let Some(arr) = kids.as_array()
        {
            for k in arr.iter().rev() {
                if let Some(r) = k.as_ref() {
                    stack.push((r, res.clone(), media.clone(), crop.clone(), depth + 1));
                }
            }
            continue;
        }
        let media_r = num_rect(cos, media.as_ref()).unwrap_or(Rect::new(0.0, 0.0, 612.0, 792.0));
        let box_ = num_rect(cos, crop.as_ref())
            .map(|c| {
                Rect::new(
                    c.x0.max(media_r.x0),
                    c.y0.max(media_r.y0),
                    c.x1.min(media_r.x1),
                    c.y1.min(media_r.y1),
                )
            })
            .filter(|r| r.width() > 0.0 && r.height() > 0.0)
            .unwrap_or(media_r);
        out.push(SrcPage {
            dict: d,
            resources: res,
            box_,
        });
    }
    Ok(out)
}

/// Copies objects from a source document, following references (never `/Parent`).
struct Importer<'a> {
    src: &'a CosDoc,
    map: HashMap<ObjRef, ObjRef>,
    queue: Vec<ObjRef>,
    copied: usize,
}

impl<'a> Importer<'a> {
    fn new(src: &'a CosDoc) -> Self {
        Self {
            src,
            map: HashMap::new(),
            queue: Vec::new(),
            copied: 0,
        }
    }

    fn rewrite(&mut self, dst: &mut CosDoc, o: &Object, depth: usize) -> Object {
        if depth > MAX_DEPTH {
            return Object::Null;
        }
        match o {
            Object::Ref(r) => match self.map.get(r) {
                Some(n) => Object::Ref(*n),
                None => {
                    let n = dst.add(Object::Null);
                    self.map.insert(*r, n);
                    self.queue.push(*r);
                    Object::Ref(n)
                }
            },
            Object::Array(a) => Object::Array(a.iter().map(|x| self.rewrite(dst, x, depth + 1)).collect()),
            Object::Dict(d) => Object::Dict(self.rewrite_dict(dst, d, depth)),
            Object::Stream(s) => Object::Stream(Stream {
                dict: self.rewrite_dict(dst, &s.dict, depth),
                raw: s.raw.clone(),
            }),
            other => other.clone(),
        }
    }

    fn rewrite_dict(&mut self, dst: &mut CosDoc, d: &Dict, depth: usize) -> Dict {
        d.iter()
            .filter(|(k, _)| k.as_slice() != b"Parent")
            .map(|(k, v)| (k.clone(), self.rewrite(dst, v, depth + 1)))
            .collect()
    }

    fn drain(&mut self, dst: &mut CosDoc) -> Result<()> {
        while let Some(r) = self.queue.pop() {
            self.copied += 1;
            if self.copied > MAX_OBJECTS {
                return Err(invalid("the overlaid pages reference too many objects"));
            }
            let Some(new) = self.map.get(&r).copied() else { continue };
            let obj = self.src.get(r);
            let out = self.rewrite(dst, &obj, 0);
            dst.set(new, out);
        }
        Ok(())
    }
}

/// The page's content streams, decoded and joined.
fn page_content(cos: &CosDoc, page: &Dict) -> Result<Vec<u8>> {
    let contents = page.get(b"Contents").map(|c| cos.resolve(c));
    let mut streams = Vec::new();
    match contents.as_deref() {
        Some(Object::Array(a)) => streams.extend(a.iter().map(|x| cos.resolve(x))),
        Some(Object::Stream(_)) => streams.extend(contents.clone()),
        _ => {}
    }
    let mut out = Vec::new();
    for s in streams {
        if let Object::Stream(st) = &*s {
            let data = st.decoded()?;
            if out.len().saturating_add(data.len()) > MAX_CONTENT {
                return Err(invalid("a page's content is too large to overlay"));
            }
            out.extend_from_slice(&data);
            out.push(b'\n');
        }
    }
    Ok(out)
}

fn nums(v: &[f64]) -> Object {
    Object::Array(v.iter().map(|x| Object::Real(*x)).collect())
}

fn name(s: &str) -> Object {
    Object::Name(s.as_bytes().to_vec())
}

fn fmt(v: f64) -> String {
    let s = format!("{:.5}", if v.abs() < 1e-9 { 0.0 } else { v });
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".into()
    } else {
        s.into()
    }
}

/// Compose the overlay and write it to `out` (atomic). The first layer gives the page boxes.
pub fn overlay_pages(layers: &[OverlayLayer], out: &Path) -> Result<OverlayReport> {
    if layers.len() < 2 {
        return Err(invalid("an overlay needs at least two layers"));
    }
    if layers.len() > MAX_LAYERS {
        return Err(invalid(format!("at most {MAX_LAYERS} layers")));
    }
    let mut sources = Vec::with_capacity(layers.len());
    for (i, l) in layers.iter().enumerate() {
        if !(l.opacity.is_finite() && (0.0..=1.0).contains(&l.opacity)) {
            return Err(invalid("opacity must be from 0 to 1"));
        }
        let cos = CosDoc::open(l.bytes.clone()).map_err(|e| invalid(format!("layer {}: {e}", i + 1)))?;
        let pages = source_pages(&cos)?;
        if pages.is_empty() {
            return Err(invalid(format!("layer {} has no pages", i + 1)));
        }
        sources.push((cos, pages));
    }
    // Output page count: the layers' page lists (all equal), or the shortest document.
    let count = match layers.iter().find(|l| !l.pages.is_empty()) {
        Some(l) => l.pages.len(),
        None => sources.iter().map(|(_, p)| p.len()).min().unwrap_or(0),
    };
    if count == 0 || count > MAX_PAGES {
        return Err(invalid(format!("an overlay has 1 to {MAX_PAGES} pages")));
    }
    for (i, l) in layers.iter().enumerate() {
        let n = sources.get(i).map_or(0, |s| s.1.len());
        if !l.pages.is_empty() && l.pages.len() != count {
            return Err(invalid("every layer must list the same number of pages"));
        }
        if let Some(p) = l.pages.iter().find(|p| **p >= n) {
            return Err(EngineError::NoPage { page: p + 1, count: n });
        }
    }
    let page_of =
        |layer: usize, k: usize| -> usize { layers.get(layer).and_then(|l| l.pages.get(k).copied()).unwrap_or(k) };

    // The new document: blank pages sized like the first layer's pages.
    let sizes: Vec<(f64, f64)> = (0..count)
        .map(|k| {
            sources
                .first()
                .and_then(|(_, p)| p.get(page_of(0, k)))
                .map_or((612.0, 792.0), |p| (p.box_.width(), p.box_.height()))
        })
        .collect();
    for (w, h) in &sizes {
        crate::blank::check_size(*w, *h)?;
    }
    let mut dst = CosDoc::open(Arc::new(crate::blank::pdf_bytes(&sizes)?))?;
    let out_pages: Vec<ObjRef> = {
        let root = dst.root().ok_or_else(|| invalid("no catalog"))?;
        let pages = dst
            .dict(&Object::Ref(root))
            .and_then(|d| d.reference(b"Pages"))
            .ok_or_else(|| invalid("no pages"))?;
        let kids = dst.dict(&Object::Ref(pages)).and_then(|d| d.get(b"Kids").cloned());
        kids.and_then(|k| k.as_array().map(|a| a.iter().filter_map(Object::as_ref).collect()))
            .unwrap_or_default()
    };

    // One optional content group per layer.
    let names: Vec<String> = layers
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let n: String = l.name.chars().take(200).collect();
            if n.trim().is_empty() {
                format!("Layer {}", i + 1)
            } else {
                n
            }
        })
        .collect();
    let ocgs: Vec<ObjRef> = names
        .iter()
        .map(|n| {
            let mut d = Dict::new();
            d.set(b"Type".to_vec(), name("OCG"));
            d.set(b"Name".to_vec(), Object::String(PdfString::text(n)));
            dst.add(Object::Dict(d))
        })
        .collect();

    let mut importers: Vec<Importer> = sources.iter().map(|(c, _)| Importer::new(c)).collect();
    for (k, page_ref) in out_pages.iter().enumerate().take(count) {
        let base = sources
            .first()
            .and_then(|(_, p)| p.get(page_of(0, k)))
            .map(|p| p.box_)
            .ok_or_else(|| invalid("no first page"))?;
        // The new page's user space has its origin at the base crop box's lower-left corner.
        let to_page: Matrix = [1.0, 0.0, 0.0, 1.0, -base.x0, -base.y0];
        let (w, h) = (base.width(), base.height());
        let mut content = String::new();
        let mut xobjects = Dict::new();
        let mut gstates = Dict::new();
        let mut props = Dict::new();
        for (i, l) in layers.iter().enumerate() {
            let (Some((src_cos, src_pages)), Some(imp)) = (sources.get(i), importers.get_mut(i)) else {
                continue;
            };
            let Some(sp) = src_pages.get(page_of(i, k)) else {
                continue;
            };
            let align: Matrix = match l.align {
                OverlayAlign::Page => [1.0, 0.0, 0.0, 1.0, base.x0 - sp.box_.x0, base.y0 - sp.box_.y0],
                OverlayAlign::Bounds => {
                    let sx = base.width() / sp.box_.width().max(1e-6);
                    let sy = base.height() / sp.box_.height().max(1e-6);
                    [sx, 0.0, 0.0, sy, base.x0 - sp.box_.x0 * sx, base.y0 - sp.box_.y0 * sy]
                }
                OverlayAlign::Points { from, to } => two_point_matrix(from, to)
                    .ok_or_else(|| invalid("alignment points must be two distinct points on each page"))?,
            };
            let m = then(align, to_page);
            // The source page as a form XObject in its own user space.
            let data = page_content(src_cos, &sp.dict)?;
            let mut fd = Dict::new();
            fd.set(b"Type".to_vec(), name("XObject"));
            fd.set(b"Subtype".to_vec(), name("Form"));
            fd.set(
                b"BBox".to_vec(),
                nums(&[sp.box_.x0, sp.box_.y0, sp.box_.x1, sp.box_.y1]),
            );
            if let Some(res) = &sp.resources {
                let r = imp.rewrite(&mut dst, res, 0);
                fd.set(b"Resources".to_vec(), r);
            }
            imp.drain(&mut dst)?;
            let page_form = dst.add(Object::Stream(Stream::flate(fd, &data)));

            // The recoloured layer: an isolated group on white, the colour laid over with Lighten.
            let c = l.color;
            let group_content = format!(
                "1 g 0 0 {w} {h} re f\nq {} cm /P Do Q\n/L gs {} {} {} rg 0 0 {w} {h} re f\n",
                m.iter().map(|v| fmt(*v)).collect::<Vec<_>>().join(" "),
                fmt(c.r.clamp(0.0, 1.0)),
                fmt(c.g.clamp(0.0, 1.0)),
                fmt(c.b.clamp(0.0, 1.0)),
                w = fmt(w),
                h = fmt(h),
            );
            let mut gd = Dict::new();
            gd.set(b"Type".to_vec(), name("XObject"));
            gd.set(b"Subtype".to_vec(), name("Form"));
            gd.set(b"BBox".to_vec(), nums(&[0.0, 0.0, w, h]));
            let mut group = Dict::new();
            group.set(b"S".to_vec(), name("Transparency"));
            group.set(b"I".to_vec(), Object::Bool(true));
            group.set(b"CS".to_vec(), name("DeviceRGB"));
            gd.set(b"Group".to_vec(), Object::Dict(group));
            let mut res = Dict::new();
            let mut xo = Dict::new();
            xo.set(b"P".to_vec(), Object::Ref(page_form));
            res.set(b"XObject".to_vec(), Object::Dict(xo));
            let mut lighten = Dict::new();
            lighten.set(b"BM".to_vec(), name("Lighten"));
            let mut gs = Dict::new();
            gs.set(b"L".to_vec(), Object::Dict(lighten));
            res.set(b"ExtGState".to_vec(), Object::Dict(gs));
            gd.set(b"Resources".to_vec(), Object::Dict(res));
            let layer_form = dst.add(Object::Stream(Stream::flate(gd, group_content.as_bytes())));

            let mut multiply = Dict::new();
            multiply.set(b"BM".to_vec(), name("Multiply"));
            multiply.set(b"ca".to_vec(), Object::Real(l.opacity));
            multiply.set(b"CA".to_vec(), Object::Real(l.opacity));
            xobjects.set(format!("L{i}").into_bytes(), Object::Ref(layer_form));
            gstates.set(format!("G{i}").into_bytes(), Object::Dict(multiply));
            if let Some(oc) = ocgs.get(i) {
                props.set(format!("OC{i}").into_bytes(), Object::Ref(*oc));
            }
            content.push_str(&format!("/OC /OC{i} BDC q /G{i} gs /L{i} Do Q EMC\n"));
        }
        let mut res = Dict::new();
        res.set(b"XObject".to_vec(), Object::Dict(xobjects));
        res.set(b"ExtGState".to_vec(), Object::Dict(gstates));
        res.set(b"Properties".to_vec(), Object::Dict(props));
        let contents = dst.add(Object::Stream(Stream::flate(Dict::new(), content.as_bytes())));
        dst.update_dict(*page_ref, |d| {
            d.set(b"Resources".to_vec(), Object::Dict(res));
            d.set(b"Contents".to_vec(), Object::Ref(contents));
            d.set(b"MediaBox".to_vec(), nums(&[0.0, 0.0, w, h]));
        })?;
    }

    // The layers in the Layers panel, all visible.
    let root = dst.root().ok_or_else(|| invalid("no catalog"))?;
    let refs: Vec<Object> = ocgs.iter().map(|r| Object::Ref(*r)).collect();
    let mut d = Dict::new();
    d.set(b"Name".to_vec(), Object::String(PdfString::text("Overlay")));
    d.set(b"Order".to_vec(), Object::Array(refs.clone()));
    d.set(b"ON".to_vec(), Object::Array(refs.clone()));
    let mut ocp = Dict::new();
    ocp.set(b"OCGs".to_vec(), Object::Array(refs));
    ocp.set(b"D".to_vec(), Object::Dict(d));
    dst.update_dict(root, |c| c.set(b"OCProperties".to_vec(), Object::Dict(ocp)))?;

    let opts = SaveOptions {
        mod_date: Some(markupcraft_revu::pdf_date_now()),
        ..Default::default()
    };
    let bytes = write_full(&dst, &opts)?;
    crate::write_atomic(out, &bytes)?;
    Ok(OverlayReport {
        pages: count,
        layers: names,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raster::Renderable;
    use crate::synthetic::{SyntheticPage, line, pdf};

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("markupcraft-overlay-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d.join(name)
    }

    fn layer(bytes: Vec<u8>, color: Color, align: OverlayAlign) -> OverlayLayer {
        OverlayLayer {
            bytes: Arc::new(bytes),
            pages: Vec::new(),
            color,
            opacity: 1.0,
            name: String::new(),
            align,
        }
    }

    #[test]
    fn two_points_give_scale_rotation_and_offset() {
        let m = two_point_matrix(
            [Point::new(0.0, 0.0), Point::new(10.0, 0.0)],
            [Point::new(5.0, 5.0), Point::new(5.0, 25.0)],
        )
        .unwrap();
        // (10, 0) -> (5, 25): scale 2, a quarter turn, offset (5, 5).
        let (x, y) = (m[0] * 10.0 + m[4], m[1] * 10.0 + m[5]);
        assert!((x - 5.0).abs() < 1e-9 && (y - 25.0).abs() < 1e-9, "{m:?}");
        assert!(two_point_matrix([Point::new(1.0, 1.0); 2], [Point::new(0.0, 0.0); 2]).is_none());
    }

    #[test]
    fn overlay_recolours_layers_and_keeps_them_toggleable() {
        let common = line(50.0, 100.0, 550.0, 100.0, 6.0);
        let a = pdf(&[SyntheticPage::new(
            612.0,
            792.0,
            format!("{common}{}", line(100.0, 400.0, 500.0, 400.0, 6.0)),
        )]);
        // B is the same sheet on a larger page, shifted by (100, 50): page alignment by two points.
        let b = pdf(&[SyntheticPage::new(
            800.0,
            900.0,
            format!(
                "{}{}",
                line(150.0, 150.0, 650.0, 150.0, 6.0),
                line(200.0, 650.0, 600.0, 650.0, 6.0)
            ),
        )]);
        let out = tmp("overlay.pdf");
        let shift = OverlayAlign::Points {
            from: [Point::new(150.0, 150.0), Point::new(650.0, 150.0)],
            to: [Point::new(50.0, 100.0), Point::new(550.0, 100.0)],
        };
        let r = overlay_pages(
            &[
                layer(a, default_color(0), OverlayAlign::Page),
                layer(b, default_color(1), shift),
            ],
            &out,
        )
        .unwrap();
        assert_eq!(r.pages, 1);
        assert_eq!(r.layers, vec!["Layer 1", "Layer 2"]);

        let bytes = std::fs::read(&out).unwrap();
        let cos = CosDoc::open(Arc::new(bytes.clone())).unwrap();
        let cat = cos.dict(&Object::Ref(cos.root().unwrap())).unwrap();
        let ocp = cos.dict(cat.get(b"OCProperties").unwrap()).unwrap();
        assert_eq!(cos.resolve(ocp.get(b"OCGs").unwrap()).as_array().unwrap().len(), 2);

        let img = Renderable::new(Arc::new(bytes), false)
            .unwrap()
            .render_rgba(0, 1.0)
            .unwrap();
        let at = |x: f64, y: f64| img.pixel(x, y);
        let [r0, g0, b0] = at(300.0, 100.0); // both layers: dark
        assert!(r0 < 90 && g0 < 90 && b0 < 120, "common line {:?}", [r0, g0, b0]);
        let [r1, g1, b1] = at(300.0, 400.0); // only A: red
        assert!(r1 > 180 && g1 < 90 && b1 < 90, "A only {:?}", [r1, g1, b1]);
        let [r2, _, b2] = at(300.0, 600.0); // only B (650 - 50): blue
        assert!(b2 > 180 && r2 < 90, "B only {:?}", [r2, b2]);
        let [rw, gw, bw] = at(300.0, 250.0); // paper stays white
        assert!(rw > 240 && gw > 240 && bw > 240);

        // Bad input is an error.
        assert!(overlay_pages(&[layer(b"junk".to_vec(), Color::RED, OverlayAlign::Page)], &out).is_err());
        let one = pdf(&[SyntheticPage::new(100.0, 100.0, "")]);
        let mut l = layer(one.clone(), Color::RED, OverlayAlign::Bounds);
        l.pages = vec![3];
        assert!(overlay_pages(&[l, layer(one, Color::RED, OverlayAlign::Page)], &out).is_err());
    }
}
