//! Page marks drawn into page content: headers and footers (with page, date and Bates tokens),
//! text watermarks, Bates numbering, and removing them again. Built on PdfCraft's page-mark
//! editor (`pdfcraft-edit`): each mark is its own tagged content stream, so it can be found,
//! replaced or removed later without touching the drawing underneath. Markups are unaffected.

use markupcraft_model::Color;
pub use pdfcraft_edit::MarkKind;

use crate::docutil::{err, today};
use crate::{Result, Session, invalid};

/// Header and footer settings. Tokens in the texts: `<<1>>` page number, `<<n>>` page count,
/// `<<1 of n>>`, `<<Page 1 of n>>`, dates `<<m/d/yyyy>>`, `<<yyyy-mm-dd>>`, ..., and Bates
/// numbers `<<Bates Number#digits#start#prefix#suffix>>`; file data `<<File Name>>`,
/// `<<File Path>>`, `<<Author>>`, `<<Title>>`, `<<Subject>>` (see `hf_tokens`).
#[derive(Debug, Clone, PartialEq)]
pub struct HeaderFooter {
    /// Left, centre and right header, then left, centre and right footer.
    pub text: [String; 6],
    pub font_size: f64,
    pub color: Color,
    pub underline: bool,
    /// Distance from the page edges in points: top, bottom, left, right.
    pub margins: [f64; 4],
    /// The number `<<1>>` shows on the first page.
    pub start_number: u32,
}

impl Default for HeaderFooter {
    fn default() -> Self {
        let d = pdfcraft_edit::HeaderFooter::default();
        Self {
            text: Default::default(),
            font_size: d.font_size,
            color: Color::BLACK,
            underline: false,
            margins: d.margins,
            start_number: 1,
        }
    }
}

/// A text watermark.
#[derive(Debug, Clone, PartialEq)]
pub struct Watermark {
    pub text: String,
    /// 0 = fit about half the page diagonal.
    pub font_size: f64,
    pub color: Color,
    /// 0 to 1.
    pub opacity: f64,
    /// Degrees counter-clockwise.
    pub rotation: f64,
    /// Behind the page content instead of on top.
    pub behind: bool,
    /// Offset of the centre from the page centre, points (right, up).
    pub offset: [f64; 2],
}

impl Default for Watermark {
    fn default() -> Self {
        let d = pdfcraft_edit::Watermark::default();
        Self {
            text: String::new(),
            font_size: 0.0,
            color: Color::rgb(d.color[0], d.color[1], d.color[2]),
            opacity: d.opacity,
            rotation: d.rotation,
            behind: false,
            offset: [0.0; 2],
        }
    }
}

/// Where a Bates number goes: one of the six header/footer slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    HeaderLeft,
    HeaderCenter,
    HeaderRight,
    FooterLeft,
    FooterCenter,
    FooterRight,
}

impl Slot {
    pub fn from_name(s: &str) -> Option<Slot> {
        Some(match s.to_ascii_lowercase().replace(['-', ' '], "_").as_str() {
            "header_left" | "top_left" => Slot::HeaderLeft,
            "header_center" | "header_centre" | "top_center" => Slot::HeaderCenter,
            "header_right" | "top_right" => Slot::HeaderRight,
            "footer_left" | "bottom_left" => Slot::FooterLeft,
            "footer_center" | "footer_centre" | "bottom_center" => Slot::FooterCenter,
            "footer_right" | "bottom_right" => Slot::FooterRight,
            _ => return None,
        })
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// Bates numbering settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Bates {
    pub prefix: String,
    pub suffix: String,
    /// Digits (zero padded), 1 to 15.
    pub digits: usize,
    pub start: u64,
    pub slot: Slot,
    pub font_size: f64,
    pub color: Color,
    pub margins: [f64; 4],
}

impl Default for Bates {
    fn default() -> Self {
        Self {
            prefix: String::new(),
            suffix: String::new(),
            digits: 6,
            start: 1,
            slot: Slot::FooterRight,
            font_size: 10.0,
            color: Color::BLACK,
            margins: pdfcraft_edit::HeaderFooter::default().margins,
        }
    }
}

fn rgb(c: Color) -> [f64; 3] {
    [c.r, c.g, c.b]
}

fn check_text(s: &str) -> Result<()> {
    if s.chars().count() > 2_000 {
        return Err(invalid("the text is too long (2000 characters at most)"));
    }
    Ok(())
}

impl Session {
    fn check_mark_pages(&self, pages: &[usize]) -> Result<()> {
        if pages.is_empty() {
            return Err(invalid("no pages given"));
        }
        for p in pages {
            self.page(*p)?;
        }
        Ok(())
    }

    /// Add a header and footer to `pages`; `replace` removes existing ones on them first.
    pub fn add_header_footer(&mut self, pages: &[usize], hf: &HeaderFooter, replace: bool) -> Result<()> {
        self.check_mark_pages(pages)?;
        for t in &hf.text {
            check_text(t)?;
        }
        let settings = pdfcraft_edit::HeaderFooter {
            text: hf.text.clone().map(|t| self.expand_file_tokens(&t)),
            font_size: hf.font_size,
            color: rgb(hf.color),
            underline: hf.underline,
            margins: hf.margins,
            start_number: hf.start_number.max(1),
        };
        let cx = pdfcraft_edit::Context { date: today() };
        self.cos_edit("Header & Footer", |cos| {
            pdfcraft_edit::add_header_footer(cos, pages, &settings, replace, &cx).map_err(err)?;
            Ok(((), true))
        })
    }

    /// Add a text watermark to `pages`; `replace` removes existing watermarks on them first.
    pub fn add_watermark(&mut self, pages: &[usize], wm: &Watermark, replace: bool) -> Result<()> {
        self.check_mark_pages(pages)?;
        check_text(&wm.text)?;
        if !(wm.font_size.is_finite() && (0.0..=1_000.0).contains(&wm.font_size)) {
            return Err(invalid("the font size must be 0 (fit) to 1000"));
        }
        let settings = pdfcraft_edit::Watermark {
            text: wm.text.clone(),
            source: None,
            scale: 0.5,
            font_size: wm.font_size,
            color: rgb(wm.color),
            opacity: wm.opacity,
            rotation: wm.rotation,
            behind: wm.behind,
            offset: wm.offset,
        };
        self.cos_edit("Watermark", |cos| {
            pdfcraft_edit::add_watermark(cos, pages, &settings, replace).map_err(err)?;
            Ok(((), true))
        })
    }

    /// Number `pages` with Bates numbers (`prefix` + zero-padded number + `suffix`), counting
    /// up from `start` in page order. Bates numbers are header/footer marks: replacing or
    /// removing headers and footers also affects them. Returns the first and last number.
    pub fn add_bates(&mut self, pages: &[usize], b: &Bates, replace: bool) -> Result<(String, String)> {
        self.check_mark_pages(pages)?;
        check_text(&b.prefix)?;
        check_text(&b.suffix)?;
        if !(1..=15).contains(&b.digits) {
            return Err(invalid("digits must be 1 to 15"));
        }
        if b.prefix.contains(['#', '<', '>']) || b.suffix.contains(['#', '<', '>']) {
            return Err(invalid("the prefix and suffix may not contain #, < or >"));
        }
        let max = 10u64.saturating_pow(b.digits as u32);
        let last = b.start.saturating_add(pages.len() as u64).saturating_sub(1);
        if last >= max {
            return Err(invalid(format!(
                "{} pages from {} need more than {} digits",
                pages.len(),
                b.start,
                b.digits
            )));
        }
        let mut text: [String; 6] = Default::default();
        if let Some(slot) = text.get_mut(b.slot.index()) {
            *slot = format!("<<Bates Number#{}#{}#{}#{}>>", b.digits, b.start, b.prefix, b.suffix);
        }
        let hf = HeaderFooter {
            text,
            font_size: b.font_size,
            color: b.color,
            underline: false,
            margins: b.margins,
            start_number: 1,
        };
        self.add_header_footer(pages, &hf, replace)?;
        let fmt = |n: u64| format!("{}{:0width$}{}", b.prefix, n, b.suffix, width = b.digits);
        Ok((fmt(b.start), fmt(last)))
    }

    /// Remove marks of `kind` from `pages`; returns how many were removed.
    pub fn remove_marks(&mut self, pages: &[usize], kind: MarkKind) -> Result<usize> {
        self.check_mark_pages(pages)?;
        self.cos_edit("Remove Marks", |cos| {
            let n = pdfcraft_edit::remove_marks(cos, pages, kind).map_err(err)?;
            Ok((n, n > 0))
        })
    }

    /// Which kinds of marks the document has.
    pub fn marks_present(&self) -> Vec<MarkKind> {
        pdfcraft_edit::marks_present(&self.file.cos)
    }
}
