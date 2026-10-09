//! Batch > Sign & Seal: across many files, add a date, place a professional seal image, then
//! digitally sign (in the signature field of a given name when the file has one, else at a set
//! position on a set page) or certify. Each file is saved signed in place or as a copy in an
//! output folder. A file that fails is reported, not fatal.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use markupcraft_model::{Color, Kind, Markup, Rect};

use crate::signatures::{SignRequest, open_digital_id};
use crate::stamps::{StampPlace, StampSource, format_time, now_secs};
use crate::{Result, Session, invalid};

/// Most files signed in one batch.
pub const MAX_FILES: usize = 2_000;

/// A date written on each file.
#[derive(Debug, Clone, PartialEq)]
pub struct DateText {
    /// Stamp time letters (`yyyy-MM-dd`, `MMMM d, yyyy`...).
    pub format: String,
    pub rect: Rect,
}

/// What Sign & Seal does to every file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BatchSign {
    /// The digital ID (PKCS #12) and its password.
    pub p12: Vec<u8>,
    pub password: String,
    /// Sign the empty signature field of this name when a file has one.
    pub field: Option<String>,
    /// Else: a new signature on this page (0-based; past the end = the last page) ...
    pub page: usize,
    /// ... in this box (`None` = an invisible signature).
    pub rect: Option<Rect>,
    /// A seal image (PNG, JPEG or a PDF page) placed as an image stamp ...
    pub seal: Option<PathBuf>,
    /// ... in this box (default: the signature box).
    pub seal_rect: Option<Rect>,
    pub date: Option<DateText>,
    /// Certify instead of approval-signing (1 no changes, 2 forms, 3 also comments).
    pub certify: Option<u8>,
    pub reason: Option<String>,
    pub location: Option<String>,
    /// Write signed copies here (with `suffix`); `None` signs every file in place.
    pub out_dir: Option<PathBuf>,
    pub suffix: String,
}

/// One file's outcome.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SignOutcome {
    pub file: PathBuf,
    pub output: PathBuf,
    /// The field signed (existing or new).
    pub field: String,
    pub error: String,
}

fn sign_one(file: &Path, job: &BatchSign, id: &pdfcraft_sign::DigitalId, when: i64) -> Result<(PathBuf, String)> {
    let mut s = Session::open(file)?;
    let last = s.page_count().saturating_sub(1);
    let page = job.page.min(last);
    let existing = job.field.as_ref().and_then(|name| {
        s.form_fields()
            .into_iter()
            .find(|f| f.kind == "signature" && f.name == *name && f.value.is_empty())
    });
    // The seal and the date go where the signature goes.
    let (seal_page, sign_rect) = match &existing {
        Some(f) => (f.page.unwrap_or(page), f.rect),
        None => (page, job.rect),
    };
    if let Some(seal) = &job.seal {
        let r = job
            .seal_rect
            .or(sign_rect)
            .ok_or_else(|| invalid("give the seal a box (or sign in a visible box)"))?;
        s.place_stamp(
            seal_page,
            StampPlace::Rect(r),
            &StampSource::File {
                path: seal.clone(),
                page: 0,
            },
            None,
            &BTreeMap::new(),
            None,
        )?;
    }
    if let Some(d) = &job.date {
        let text = format_time(
            when,
            if d.format.trim().is_empty() {
                "yyyy-MM-dd"
            } else {
                &d.format
            },
        );
        let mut m = Markup::new(Kind::Text, seal_page, d.rect.normalized().corners().to_vec());
        m.contents = text;
        m.subject = "Date".into();
        m.color = Color::BLACK;
        m.text.color = Color::BLACK;
        m.line_width = 0.0;
        s.add_markup(m)?;
    }
    let out = match &job.out_dir {
        Some(dir) => {
            let stem = file
                .file_stem()
                .map(|x| x.to_string_lossy().into_owned())
                .unwrap_or_else(|| "signed".into());
            dir.join(format!("{stem}{}.pdf", job.suffix))
        }
        None => file.to_path_buf(),
    };
    let req = SignRequest {
        field: existing.as_ref().map(|f| f.name.clone()),
        page,
        rect: if existing.is_some() { None } else { job.rect },
        reason: job.reason.clone(),
        location: job.location.clone(),
        contact: None,
        certify: job.certify,
        image: None,
    };
    s.sign(id, &req, &out)?;
    let field = existing.map(|f| f.name).unwrap_or_else(|| {
        s.form_fields()
            .into_iter()
            .filter(|f| f.kind == "signature")
            .map(|f| f.name)
            .next_back()
            .unwrap_or_default()
    });
    Ok((out, field))
}

/// Sign and seal every file. Returns one outcome per file.
pub fn batch_sign(files: &[PathBuf], job: &BatchSign) -> Result<Vec<SignOutcome>> {
    if files.is_empty() || files.len() > MAX_FILES {
        return Err(invalid(format!("sign 1 to {MAX_FILES} files")));
    }
    if let Some(dir) = &job.out_dir
        && !dir.is_dir()
    {
        return Err(invalid(format!("{} is not a folder", dir.display())));
    }
    if job.suffix.chars().any(|c| "/\\:*?\"<>|".contains(c) || c.is_control()) {
        return Err(invalid("the suffix cannot hold path or wildcard characters"));
    }
    if job.out_dir.is_none() && !job.suffix.is_empty() {
        return Err(invalid("a suffix needs an output folder"));
    }
    let id = open_digital_id(&job.p12, &job.password)?;
    let when = now_secs();
    Ok(files
        .iter()
        .map(|f| match sign_one(f, job, &id, when) {
            Ok((output, field)) => SignOutcome {
                file: f.clone(),
                output,
                field,
                error: String::new(),
            },
            Err(e) => SignOutcome {
                file: f.clone(),
                output: PathBuf::new(),
                field: String::new(),
                error: e.to_string(),
            },
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signatures::{IdentityInfo, create_digital_id};
    use crate::synthetic::{SyntheticPage, pdf, text};

    #[test]
    fn many_files_get_a_seal_a_date_and_a_signature() {
        let d = std::env::temp_dir().join(format!("markupcraft-batchsign-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("out")).unwrap();
        for n in ["a", "b"] {
            std::fs::write(
                d.join(format!("{n}.pdf")),
                pdf(&[
                    SyntheticPage::new(612.0, 792.0, text(72.0, 700.0, 12.0, "Cover")),
                    SyntheticPage::new(612.0, 792.0, text(72.0, 700.0, 12.0, "Sheet")),
                ]),
            )
            .unwrap();
        }
        std::fs::write(d.join("bad.pdf"), b"not a pdf").unwrap();
        // A seal image.
        let mut img = image::RgbaImage::new(40, 40);
        for p in img.pixels_mut() {
            *p = image::Rgba([200, 0, 0, 255]);
        }
        img.save(d.join("seal.png")).unwrap();
        let p12 = create_digital_id(
            &IdentityInfo {
                name: "Engineer".into(),
                ..Default::default()
            },
            2,
            "pw",
        )
        .unwrap();
        let job = BatchSign {
            p12,
            password: "pw".into(),
            page: 99,
            rect: Some(Rect::new(400.0, 50.0, 560.0, 110.0)),
            seal: Some(d.join("seal.png")),
            seal_rect: Some(Rect::new(400.0, 120.0, 460.0, 180.0)),
            date: Some(DateText {
                format: "yyyy".into(),
                rect: Rect::new(470.0, 120.0, 560.0, 140.0),
            }),
            reason: Some("Sealed".into()),
            out_dir: Some(d.join("out")),
            suffix: " signed".into(),
            ..Default::default()
        };
        let r = batch_sign(&[d.join("a.pdf"), d.join("b.pdf"), d.join("bad.pdf")], &job).unwrap();
        assert_eq!(r.len(), 3);
        assert!(r.get(2).is_some_and(|o| !o.error.is_empty()));
        for o in r.iter().take(2) {
            assert!(o.error.is_empty(), "{o:?}");
            let s = Session::open(&o.output).unwrap();
            let sigs = s.signatures(&[]).unwrap();
            assert_eq!(sigs.len(), 1);
            assert!(sigs.first().is_some_and(|x| x.signed && x.page == Some(1)), "{sigs:?}");
            // The seal stamp and the date are on the last page.
            assert!(s.doc().markups.iter().any(|m| m.kind == Kind::Stamp && m.page == 1));
            assert!(
                s.doc()
                    .markups
                    .iter()
                    .any(|m| m.kind == Kind::Text && m.page == 1 && m.contents.len() == 4)
            );
        }
        // A bad password stops the batch before any file.
        assert!(
            batch_sign(
                &[d.join("a.pdf")],
                &BatchSign {
                    password: "no".into(),
                    ..job.clone()
                }
            )
            .is_err()
        );
    }
}
