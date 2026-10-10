//! Digital signatures: create a self-signed digital ID (PKCS #12), sign (visible or
//! invisible, optionally certifying, optionally with an image in the appearance), and list and
//! validate the signatures in a document.
//!
//! The cryptography and the PAdES B-B signing recipe are PdfCraft's `pdfcraft-sign`. Signing
//! with an image appearance follows the same recipe (an incremental save with a fixed-width
//! `/ByteRange` and a zero-filled `/Contents`, patched in place) with our own appearance.
//! Signing writes the signed file; the session then continues on it.

use std::collections::HashSet;
use std::path::Path;

use markupcraft_geom::Rect;
use markupcraft_revu::cos::{
    Dict, Document as CosDoc, ObjRef, Object, PdfString, SaveOptions, Stream, write_incremental,
};
use pdfcraft_sign::der::Time;
use pdfcraft_sign::{Certificate, DigitalId, Name, PrivateKey, SignOptions, Status, TrustStore};

use crate::{Result, Session, invalid};

/// Bytes reserved for the CMS signature beyond the certificates.
const RESERVE: usize = 8192;
/// Placeholder `/ByteRange` values: fixed width, patched after writing.
const BR_MARK: [i64; 3] = [1_111_111_111, 2_222_222_222, 3_333_333_333];
/// Largest signature image, in pixels.
const MAX_IMAGE_PIXELS: u64 = 16_000_000;

fn sign_err(e: impl std::fmt::Display) -> crate::EngineError {
    invalid(e.to_string())
}

/// Who a new digital ID names.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IdentityInfo {
    pub name: String,
    pub organization: String,
    pub unit: String,
    pub email: String,
    /// Two letters, or empty.
    pub country: String,
}

/// A new self-signed digital ID (P-256 key, valid for `years`) as a password-protected PKCS #12
/// file's bytes.
pub fn create_digital_id(who: &IdentityInfo, years: u32, password: &str) -> Result<Vec<u8>> {
    if who.name.trim().is_empty() || who.name.chars().count() > 200 {
        return Err(invalid("a digital ID needs a name (1 to 200 characters)"));
    }
    if !(1..=50).contains(&years) {
        return Err(invalid("a digital ID is valid for 1 to 50 years"));
    }
    if password.is_empty() {
        return Err(invalid("a digital ID file needs a password"));
    }
    let key = PrivateKey::generate_p256().map_err(sign_err)?;
    let name = Name::build(&who.name, &who.unit, &who.organization, &who.email, &who.country);
    let now = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // A serial from the clock and a fresh random-looking id.
    let mut serial: Vec<u8> = markupcraft_revu::new_markup_id().bytes().take(15).collect();
    serial.insert(0, 0x01);
    let cert = pdfcraft_sign::x509::Certificate::self_signed(&name, &key, Time::from_unix(now - 60), years, &serial)
        .map_err(sign_err)?;
    let id = DigitalId {
        key,
        certificate: cert,
        chain: Vec::new(),
        friendly_name: Some(who.name.clone()),
    };
    pdfcraft_sign::pkcs12::write(&id, password).map_err(sign_err)
}

/// Open a PKCS #12 digital ID.
pub fn open_digital_id(p12: &[u8], password: &str) -> Result<DigitalId> {
    pdfcraft_sign::pkcs12::open(p12, password).map_err(sign_err)
}

/// Certificates to trust (PEM or DER, or the certificate of a PKCS #12 file opened elsewhere).
pub fn load_certificates(bytes: &[u8]) -> Result<Vec<Certificate>> {
    pdfcraft_sign::x509::load_certificates(bytes).map_err(sign_err)
}

/// The public certificate of a digital ID, as PEM (to give to people who will trust it).
pub fn certificate_pem(id: &DigitalId) -> String {
    pdfcraft_sign::x509::to_pem(&id.certificate)
}

/// How to sign.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SignRequest {
    /// Sign this existing, unsigned signature field.
    pub field: Option<String>,
    /// Or a new field on this page (0-based) with this rectangle (`None`: invisible).
    pub page: usize,
    pub rect: Option<Rect>,
    pub reason: Option<String>,
    pub location: Option<String>,
    pub contact: Option<String>,
    /// Certify with these DocMDP permissions (1 no changes, 2 form fill and signing, 3 also
    /// comments).
    pub certify: Option<u8>,
    /// A PNG image shown on the left of the visible signature.
    pub image: Option<Vec<u8>>,
}

/// One signature as listed.
#[derive(Debug, Clone, PartialEq)]
pub struct SignatureSummary {
    pub field: String,
    pub signed: bool,
    pub signer: Option<String>,
    pub page: Option<usize>,
    pub rect: Option<Rect>,
    pub date: Option<String>,
    pub reason: Option<String>,
    pub location: Option<String>,
    pub certify: Option<u8>,
    /// valid, unknown or invalid
    pub status: &'static str,
    pub summary: &'static str,
    /// Changes made after signing, if any.
    pub modifications: Vec<String>,
    pub details: Vec<String>,
}

fn status_name(s: Status) -> &'static str {
    match s {
        Status::Valid => "valid",
        Status::Unknown => "unknown",
        Status::Invalid => "invalid",
    }
}

/// List and validate the signatures of a stored file.
pub fn list_signatures(bytes: &[u8], trust: &[Certificate]) -> Result<Vec<SignatureSummary>> {
    let cos = CosDoc::open(std::sync::Arc::new(bytes.to_vec()))?;
    let trust = TrustStore { certs: trust.to_vec() };
    Ok(pdfcraft_sign::pdf::list(&cos, bytes, &trust)
        .into_iter()
        .map(|s| SignatureSummary {
            summary: s.summary(),
            field: s.field,
            signed: s.signed,
            signer: s.signer,
            page: s.page,
            rect: s.rect.map(|r| Rect::new(r[0], r[1], r[2], r[3])),
            date: s.date,
            reason: s.reason,
            location: s.location,
            certify: s.certify,
            status: status_name(s.status),
            modifications: match s.modification {
                pdfcraft_sign::Modification::None => Vec::new(),
                pdfcraft_sign::Modification::Allowed(v) | pdfcraft_sign::Modification::Disallowed(v) => v,
            },
            details: s.details,
        })
        .collect())
}

impl Session {
    /// The signatures in the file as last saved, validated against `trust`.
    pub fn signatures(&self, trust: &[Certificate]) -> Result<Vec<SignatureSummary>> {
        let bytes = self.file.cos.bytes().clone();
        list_signatures(&bytes, trust)
    }

    /// Sign the document with `id` and write the signed file to `out` (atomic); the session
    /// then continues on the signed file (its undo history starts over). Unsaved markups are
    /// part of what is signed.
    pub fn sign(&mut self, id: &DigitalId, req: &SignRequest, out: &Path) -> Result<()> {
        if let Some(r) = req.rect {
            let r = r.normalized();
            if !([r.x0, r.y0, r.x1, r.y1]
                .iter()
                .all(|v| v.is_finite() && v.abs() <= crate::geometry::MAX_COORD)
                && r.width() >= 4.0
                && r.height() >= 4.0)
            {
                return Err(invalid(
                    "a visible signature needs a rectangle at least 4 points on each side",
                ));
            }
        }
        if req.field.is_none() {
            self.page(req.page)?;
        }
        if let Some(p) = req.certify
            && !(1..=3).contains(&p)
        {
            return Err(invalid(
                "certify permissions are 1 (no changes), 2 (form fill and signing) or 3 (also comments)",
            ));
        }
        for t in [&req.reason, &req.location, &req.contact].into_iter().flatten() {
            if t.chars().count() > 1000 {
                return Err(invalid("reason, location and contact are at most 1000 characters"));
            }
        }
        let mut cos = self.file.cos.clone();
        let mut doc = self.doc.clone();
        Session::flush(&mut cos, &mut doc, &self.vp_changed);
        let opts = SignOptions {
            field: req.field.clone(),
            page: req.page,
            rect: req.rect.map(|r| {
                let r = r.normalized();
                [r.x0, r.y0, r.x1, r.y1]
            }),
            new_field_name: None,
            reason: req.reason.clone(),
            location: req.location.clone(),
            contact: req.contact.clone(),
            date: markupcraft_revu::pdf_date_now(),
            certify: req.certify,
            appearance: Default::default(),
        };
        let signed = match &req.image {
            None => pdfcraft_sign::sign(&cos, id, &opts).map_err(sign_err)?,
            Some(img) => sign_with_image(&cos, id, &opts, img)?,
        };
        crate::write_atomic(out, &signed)?;
        let author = std::mem::take(&mut self.author);
        let limits = self.limits;
        let mut fresh = Session::from_bytes(signed, out)?;
        fresh.author = author;
        fresh.limits = limits;
        *self = fresh;
        Ok(())
    }
}

fn fmt(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".into()
    } else {
        s.into()
    }
}

/// An image XObject (with an alpha soft mask when it has transparency) from PNG bytes.
fn image_xobject(cos: &mut CosDoc, bytes: &[u8]) -> Result<(ObjRef, u32, u32)> {
    let img =
        image::load_from_memory(bytes).map_err(|e| invalid(format!("the signature image cannot be read: {e}")))?;
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 || (w as u64) * (h as u64) > MAX_IMAGE_PIXELS {
        return Err(invalid("the signature image must be 1 to 16 million pixels"));
    }
    let rgba = img.to_rgba8();
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    let mut alpha = Vec::with_capacity((w * h) as usize);
    for p in rgba.pixels() {
        rgb.extend_from_slice(&p.0[..3]);
        alpha.push(p.0[3]);
    }
    let dict = |cs: &str| {
        let mut d = Dict::new();
        d.set(b"Type".to_vec(), Object::Name(b"XObject".to_vec()));
        d.set(b"Subtype".to_vec(), Object::Name(b"Image".to_vec()));
        d.set(b"Width".to_vec(), Object::Int(w as i64));
        d.set(b"Height".to_vec(), Object::Int(h as i64));
        d.set(b"ColorSpace".to_vec(), Object::Name(cs.as_bytes().to_vec()));
        d.set(b"BitsPerComponent".to_vec(), Object::Int(8));
        d
    };
    let mut d = dict("DeviceRGB");
    if alpha.iter().any(|a| *a < 255) {
        let mask = cos.add(Object::Stream(Stream::flate(dict("DeviceGray"), &alpha)));
        d.set(b"SMask".to_vec(), Object::Ref(mask));
    }
    Ok((cos.add(Object::Stream(Stream::flate(d, &rgb))), w, h))
}

/// The visible signature with an image: the picture fills the left half (keeping its shape),
/// "Digitally signed by", the name and the date on the right, in Helvetica.
fn image_appearance(rect: [f64; 4], name: &str, opts: &SignOptions, image: (ObjRef, u32, u32)) -> Stream {
    use pdfcraft_fonts::{literal, win_ansi, wrap};
    let (w, h) = ((rect[2] - rect[0]).max(0.0), (rect[3] - rect[1]).max(0.0));
    let pad = (h * 0.06).clamp(1.0, 6.0);
    let mut out = Vec::new();
    // The picture, centred in the left half.
    let (iw, ih) = (image.1 as f64, image.2 as f64);
    let (bw, bh) = ((w * 0.5 - 2.0 * pad).max(1.0), (h - 2.0 * pad).max(1.0));
    let s = (bw / iw).min(bh / ih);
    let (dw, dh) = (iw * s, ih * s);
    let (dx, dy) = (pad + (bw - dw) / 2.0, pad + (bh - dh) / 2.0);
    out.extend(format!("q {} 0 0 {} {} {} cm /Img Do Q\n", fmt(dw), fmt(dh), fmt(dx), fmt(dy)).bytes());
    // The details on the right.
    let mut lines = vec!["Digitally signed by".to_string(), name.to_string()];
    if let Some(r) = opts.reason.as_deref().filter(|r| !r.trim().is_empty()) {
        lines.push(format!("Reason: {r}"));
    }
    if let Some(l) = opts.location.as_deref().filter(|l| !l.trim().is_empty()) {
        lines.push(format!("Location: {l}"));
    }
    lines.push(format!("Date: {}", pdfcraft_sign::pdf::display_date(&opts.date)));
    let right_x = w * 0.5 + pad;
    let right_w = (w - right_x - pad).max(1.0);
    let mut size = (h / (lines.len() as f64 * 1.15)).min(12.0);
    let wrapped = loop {
        let all: Vec<String> = lines.iter().flat_map(|l| wrap(l, size, right_w)).collect();
        if all.len() as f64 * size * 1.15 <= h - 2.0 * pad || size <= 3.0 {
            break all;
        }
        size *= 0.9;
    };
    out.extend_from_slice(b"BT\n0 g\n");
    let mut y = h - pad - size * 0.9;
    for l in &wrapped {
        out.extend(format!("/Helv {} Tf 1 0 0 1 {} {} Tm ", fmt(size), fmt(right_x), fmt(y)).bytes());
        out.extend(literal(&win_ansi(l)));
        out.extend_from_slice(b" Tj\n");
        y -= size * 1.15;
    }
    out.extend_from_slice(b"ET\n");
    let mut d = Dict::new();
    d.set(b"Type".to_vec(), Object::Name(b"XObject".to_vec()));
    d.set(b"Subtype".to_vec(), Object::Name(b"Form".to_vec()));
    d.set(
        b"BBox".to_vec(),
        Object::Array([0.0, 0.0, w, h].iter().map(|x| Object::Real(*x)).collect()),
    );
    let mut font = Dict::new();
    font.set(b"Type".to_vec(), Object::Name(b"Font".to_vec()));
    font.set(b"Subtype".to_vec(), Object::Name(b"Type1".to_vec()));
    font.set(b"BaseFont".to_vec(), Object::Name(b"Helvetica".to_vec()));
    font.set(b"Encoding".to_vec(), Object::Name(b"WinAnsiEncoding".to_vec()));
    let mut fonts = Dict::new();
    fonts.set(b"Helv".to_vec(), Object::Dict(font));
    let mut xo = Dict::new();
    xo.set(b"Img".to_vec(), Object::Ref(image.0));
    let mut res = Dict::new();
    res.set(b"Font".to_vec(), Object::Dict(fonts));
    res.set(b"XObject".to_vec(), Object::Dict(xo));
    d.set(b"Resources".to_vec(), Object::Dict(res));
    Stream::flate(d, &out)
}

/// The ByteRange placeholder's offset and the `/Contents` hex string's span.
fn locate(out: &[u8], reserve: usize) -> Result<(usize, (usize, usize))> {
    let mark = format!("0 {} {} {}", BR_MARK[0], BR_MARK[1], BR_MARK[2]);
    let hits: Vec<usize> = out
        .windows(mark.len())
        .enumerate()
        .filter(|(_, w)| *w == mark.as_bytes())
        .map(|(i, _)| i)
        .collect();
    let [br] = hits.as_slice() else {
        return Err(invalid("ByteRange placeholder not found"));
    };
    let br = *br;
    let start = out
        .get(..br)
        .and_then(|s| s.windows(3).rposition(|w| w == b"obj"))
        .ok_or_else(|| invalid("signature object not found"))?;
    let end = out
        .get(br..)
        .and_then(|s| s.windows(6).position(|w| w == b"endobj"))
        .map(|e| br + e)
        .ok_or_else(|| invalid("unterminated signature object"))?;
    let region = out
        .get(start..end)
        .ok_or_else(|| invalid("signature object not found"))?;
    let mut zeros = vec![b'<'];
    zeros.extend(std::iter::repeat_n(b'0', reserve * 2));
    zeros.push(b'>');
    let c = region
        .windows(zeros.len())
        .position(|w| w == zeros.as_slice())
        .ok_or_else(|| invalid("Contents placeholder not found"))?;
    Ok((br, (start + c, start + c + zeros.len())))
}

/// PdfCraft's PAdES B-B signing recipe with an image appearance.
fn sign_with_image(doc: &CosDoc, id: &DigitalId, opts: &SignOptions, image: &[u8]) -> Result<Vec<u8>> {
    if doc.security().is_some() || doc.output_handler().is_some() {
        return Err(invalid("signing encrypted documents is not supported"));
    }
    let mut doc = doc.clone();
    let root = doc.root().ok_or_else(|| invalid("the document has no catalog"))?;
    let alg = id.key.preferred_digest();
    let reserve = RESERVE + id.certificate.raw.len() + id.chain.iter().map(|c| c.raw.len()).sum::<usize>();
    let name = id.certificate.display_name();
    let mut v = Dict::new();
    v.set(b"Type".to_vec(), Object::Name(b"Sig".to_vec()));
    v.set(b"Filter".to_vec(), Object::Name(b"Adobe.PPKLite".to_vec()));
    v.set(b"SubFilter".to_vec(), Object::Name(b"ETSI.CAdES.detached".to_vec()));
    v.set(
        b"ByteRange".to_vec(),
        Object::Array([0].iter().chain(BR_MARK.iter()).map(|n| Object::Int(*n)).collect()),
    );
    v.set(
        b"Contents".to_vec(),
        Object::String(PdfString {
            bytes: vec![0; reserve],
            hex: true,
        }),
    );
    v.set(b"M".to_vec(), Object::String(PdfString::text(&opts.date)));
    v.set(b"Name".to_vec(), Object::String(PdfString::text(&name)));
    for (k, val) in [
        (&b"Reason"[..], &opts.reason),
        (b"Location", &opts.location),
        (b"ContactInfo", &opts.contact),
    ] {
        if let Some(s) = val.as_deref().filter(|s| !s.trim().is_empty()) {
            v.set(k.to_vec(), Object::String(PdfString::text(s)));
        }
    }
    if let Some(p) = opts.certify {
        let mut tp = Dict::new();
        tp.set(b"Type".to_vec(), Object::Name(b"TransformParams".to_vec()));
        tp.set(b"P".to_vec(), Object::Int(p.clamp(1, 3) as i64));
        tp.set(b"V".to_vec(), Object::Name(b"1.2".to_vec()));
        let mut sr = Dict::new();
        sr.set(b"Type".to_vec(), Object::Name(b"SigRef".to_vec()));
        sr.set(b"TransformMethod".to_vec(), Object::Name(b"DocMDP".to_vec()));
        sr.set(b"TransformParams".to_vec(), Object::Dict(tp));
        v.set(b"Reference".to_vec(), Object::Array(vec![Object::Dict(sr)]));
    }
    let sig = doc.add(Object::Dict(v));
    if opts.certify.is_some() {
        let mut perms = Dict::new();
        perms.set(b"DocMDP".to_vec(), Object::Ref(sig));
        doc.update_dict(root, |c| c.set(b"Perms".to_vec(), Object::Dict(perms)))?;
    }
    let pages = pdfcraft_annot::page_refs(&doc).map_err(sign_err)?;
    let (widget, rect) = match &opts.field {
        Some(n) => {
            let f = pdfcraft_forms::fields(&doc)
                .into_iter()
                .find(|f| &f.name == n && f.kind == pdfcraft_forms::FieldKind::Signature)
                .ok_or_else(|| invalid(format!("there is no signature field {n:?}")))?;
            if f.value.iter().any(|v| !v.is_empty()) || doc.dict(&Object::Ref(f.obj)).is_some_and(|d| d.contains(b"V"))
            {
                return Err(invalid(format!("the field {n:?} is already signed")));
            }
            let w = f.widgets.first().ok_or_else(|| invalid("the field has no widget"))?;
            doc.update_dict(f.obj, |d| d.set(b"V".to_vec(), Object::Ref(sig)))?;
            (w.obj, w.rect)
        }
        None => {
            let page = *pages
                .get(opts.page)
                .ok_or_else(|| invalid(format!("page {} does not exist", opts.page + 1)))?;
            let rect = opts
                .rect
                .ok_or_else(|| invalid("an image signature needs a rectangle"))?;
            let taken: HashSet<String> = pdfcraft_forms::fields(&doc).into_iter().map(|f| f.name).collect();
            let fname = (1..10_000)
                .map(|i| format!("Signature{i}"))
                .find(|n| !taken.contains(n))
                .unwrap_or_else(|| "Signature".into());
            let mut w = Dict::new();
            w.set(b"Type".to_vec(), Object::Name(b"Annot".to_vec()));
            w.set(b"Subtype".to_vec(), Object::Name(b"Widget".to_vec()));
            w.set(b"FT".to_vec(), Object::Name(b"Sig".to_vec()));
            w.set(b"T".to_vec(), Object::String(PdfString::text(&fname)));
            w.set(b"V".to_vec(), Object::Ref(sig));
            w.set(
                b"Rect".to_vec(),
                Object::Array(rect.iter().map(|x| Object::Real(*x)).collect()),
            );
            w.set(b"P".to_vec(), Object::Ref(page));
            w.set(b"F".to_vec(), Object::Int(4 | 128));
            let wr = doc.add(Object::Dict(w));
            let pd = doc.dict(&Object::Ref(page)).unwrap_or_default();
            match pd.get(b"Annots") {
                Some(Object::Ref(ar)) => {
                    let mut a = doc.get(*ar).as_array().cloned().unwrap_or_default();
                    a.push(Object::Ref(wr));
                    doc.set(*ar, Object::Array(a));
                }
                other => {
                    let mut a = other.and_then(|o| o.as_array().cloned()).unwrap_or_default();
                    a.push(Object::Ref(wr));
                    doc.update_dict(page, |d| d.set(b"Annots".to_vec(), Object::Array(a)))?;
                }
            }
            let catalog = doc.dict(&Object::Ref(root)).unwrap_or_default();
            let (af_ref, mut af) = match catalog.get(b"AcroForm") {
                Some(Object::Ref(r)) => (Some(*r), doc.dict(&Object::Ref(*r)).unwrap_or_default()),
                Some(Object::Dict(d)) => (None, d.clone()),
                _ => (None, Dict::new()),
            };
            let mut fields = match af.get(b"Fields") {
                Some(Object::Ref(r)) => doc.get(*r).as_array().cloned().unwrap_or_default(),
                Some(Object::Array(a)) => a.clone(),
                _ => Vec::new(),
            };
            fields.push(Object::Ref(wr));
            af.set(b"Fields".to_vec(), Object::Array(fields));
            af.set(b"SigFlags".to_vec(), Object::Int(3));
            match af_ref {
                Some(r) => doc.set(r, Object::Dict(af)),
                None => doc.update_dict(root, |c| c.set(b"AcroForm".to_vec(), Object::Dict(af)))?,
            }
            (wr, rect)
        }
    };
    if opts.field.is_some() {
        let catalog = doc.dict(&Object::Ref(root)).unwrap_or_default();
        match catalog.get(b"AcroForm") {
            Some(Object::Ref(r)) => doc.update_dict(*r, |af| af.set(b"SigFlags".to_vec(), Object::Int(3)))?,
            Some(Object::Dict(_)) => doc.update_dict(root, |c| {
                if let Some(Object::Dict(af)) = c.get_mut(b"AcroForm") {
                    af.set(b"SigFlags".to_vec(), Object::Int(3));
                }
            })?,
            _ => {}
        }
    }
    let img = image_xobject(&mut doc, image)?;
    let ap = image_appearance(rect, &name, opts, img);
    let ap_ref = doc.add(Object::Stream(ap));
    let mut apd = Dict::new();
    apd.set(b"N".to_vec(), Object::Ref(ap_ref));
    doc.update_dict(widget, |w| w.set(b"AP".to_vec(), Object::Dict(apd)))?;
    let save = SaveOptions {
        mod_date: Some(opts.date.clone()),
        object_streams: false,
        ..SaveOptions::default()
    };
    let mut out = write_incremental(&doc, &save)?;
    let (br_at, (start, end)) = locate(&out, reserve)?;
    let mut br_text = format!("0 {start} {end} {}", out.len() - end).into_bytes();
    let width = format!("0 {} {} {}", BR_MARK[0], BR_MARK[1], BR_MARK[2]).len();
    if br_text.len() > width {
        return Err(invalid("the document is too large to sign"));
    }
    br_text.resize(width, b' ');
    out.get_mut(br_at..br_at + width)
        .ok_or_else(|| invalid("ByteRange placeholder out of range"))?
        .copy_from_slice(&br_text);
    let (head, tail) = (
        out.get(..start).ok_or_else(|| invalid("bad signature range"))?,
        out.get(end..).ok_or_else(|| invalid("bad signature range"))?,
    );
    let digest = alg.digest(&[head, tail]);
    let cms = pdfcraft_sign::cms::sign_detached(&id.key, &id.certificate, &id.chain, alg, &digest).map_err(sign_err)?;
    if cms.len() > reserve {
        return Err(invalid("the signature does not fit its placeholder"));
    }
    let hex: Vec<u8> = cms.iter().flat_map(|b| format!("{b:02X}").into_bytes()).collect();
    out.get_mut(start + 1..start + 1 + hex.len())
        .ok_or_else(|| invalid("Contents placeholder out of range"))?
        .copy_from_slice(&hex);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, text};

    fn png() -> Vec<u8> {
        let mut img = image::RgbaImage::new(40, 20);
        for (x, _, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([0, 0, 200, if x < 20 { 255 } else { 0 }]);
        }
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn create_an_id_sign_validate_and_detect_tampering() {
        let dir = std::env::temp_dir().join(format!("markupcraft-sign-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let who = IdentityInfo {
            name: "Test Signer".into(),
            organization: "Example Mechanical".into(),
            email: "signer@example.com".into(),
            country: "US".into(),
            ..Default::default()
        };
        let p12 = create_digital_id(&who, 5, "secret").unwrap();
        assert!(open_digital_id(&p12, "wrong").is_err());
        let id = open_digital_id(&p12, "secret").unwrap();
        assert_eq!(id.certificate.display_name(), "Test Signer");

        let bytes = pdf(&[SyntheticPage::new(
            612.0,
            792.0,
            text(50.0, 700.0, 12.0, "Approved for construction"),
        )]);
        let path = dir.join("signed.pdf");
        let mut s = Session::from_bytes(bytes, &path).unwrap();
        assert!(s.signatures(&[]).unwrap().is_empty());
        let req = SignRequest {
            rect: Some(Rect::new(300.0, 100.0, 540.0, 160.0)),
            reason: Some("Approved".into()),
            location: Some("Site office".into()),
            image: Some(png()),
            ..Default::default()
        };
        s.sign(&id, &req, &path).unwrap();
        assert!(!s.is_dirty());
        // Untrusted self-signed: intact but "unknown"; trusted: valid.
        let list = s.signatures(&[]).unwrap();
        assert_eq!(list.len(), 1);
        let sig = &list[0];
        assert!(sig.signed);
        assert_eq!((sig.status, sig.signer.as_deref()), ("unknown", Some("Test Signer")));
        assert_eq!(sig.reason.as_deref(), Some("Approved"));
        assert_eq!(sig.page, Some(0));
        let trusted = s.signatures(std::slice::from_ref(&id.certificate)).unwrap();
        assert_eq!(trusted[0].status, "valid", "{:?}", trusted[0].details);
        // The picture shows in the left half of the signature, the paper elsewhere.
        let shot = crate::raster::Renderable::new(std::sync::Arc::new(std::fs::read(&path).unwrap()), false)
            .unwrap()
            .render_rgba(0, 1.0)
            .unwrap();
        let [r, g, b] = shot.pixel(330.0, 130.0);
        assert!(b > 150 && r < 80 && g < 80, "image pixel {:?}", [r, g, b]);
        assert_eq!(
            shot.pixel(380.0, 130.0),
            [255, 255, 255],
            "transparent part of the image"
        );

        // A second, invisible signature on top.
        s.sign(&id, &SignRequest::default(), &path).unwrap();
        let list = s.signatures(std::slice::from_ref(&id.certificate)).unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.iter().all(|x| x.status == "valid"), "{list:#?}");

        // Changing a signed byte invalidates the signature.
        let mut tampered = std::fs::read(&path).unwrap();
        let at = tampered.windows(8).position(|w| w == b"Approved").unwrap();
        tampered[at] = b'X';
        let list = list_signatures(&tampered, std::slice::from_ref(&id.certificate)).unwrap();
        assert!(list.iter().any(|x| x.status == "invalid"), "{list:#?}");

        // Bad requests.
        let bad = SignRequest {
            certify: Some(7),
            ..Default::default()
        };
        assert!(s.sign(&id, &bad, &path).is_err());
        let tiny = SignRequest {
            rect: Some(Rect::new(0.0, 0.0, 1.0, 1.0)),
            ..Default::default()
        };
        assert!(s.sign(&id, &tiny, &path).is_err());
        let junk_image = SignRequest {
            rect: Some(Rect::new(300.0, 300.0, 400.0, 340.0)),
            image: Some(b"not an image".to_vec()),
            ..Default::default()
        };
        assert!(s.sign(&id, &junk_image, &path).is_err());
        assert!(create_digital_id(&IdentityInfo::default(), 5, "x").is_err());
        assert!(create_digital_id(&who, 5, "").is_err());
    }
}
