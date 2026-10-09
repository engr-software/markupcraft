//! File > Export: pages as images (PNG, JPEG, TIFF, BMP), the document as plain text, HTML,
//! RTF, Word, Excel or PowerPoint, and a page region (a schedule) as an Excel table.
//!
//! - Images are rendered from the document as it is now (markups included unless asked not to).
//! - Text, HTML, RTF and Word come from the page text through PdfCraft's exporter
//!   (`pdfcraft-export`, MIT OR Apache-2.0): paragraphs in reading order, headings by size.
//! - Excel: one sheet per page, the words of the page laid out in rows and columns by their
//!   positions (a schedule's grid), numbers as numbers. A region export does the same for the
//!   words inside a box.
//! - PowerPoint: one slide per page with the page as a picture.
//!
//! Nothing here changes the document.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use markupcraft_geom::Rect;

use crate::raster::{PageWord, Renderable, words};
use crate::{Result, Session, invalid, write_atomic};

/// Most pages one export renders.
pub const MAX_PAGES: usize = 2_000;

/// An image file format for page export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    /// JPEG with a quality from 1 to 100.
    Jpeg(u8),
    Tiff,
    Bmp,
    /// 256 colours (`finish::imaging::gif_encode`).
    Gif,
}

impl ImageFormat {
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().trim_start_matches('.') {
            "png" => Self::Png,
            "jpg" | "jpeg" => Self::Jpeg(85),
            "tif" | "tiff" => Self::Tiff,
            "bmp" => Self::Bmp,
            "gif" => Self::Gif,
            _ => return None,
        })
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg(_) => "jpg",
            Self::Tiff => "tif",
            Self::Bmp => "bmp",
            Self::Gif => "gif",
        }
    }
}

/// How pages are exported as images.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageExport {
    pub format: ImageFormat,
    /// Pixels per inch (18 to 1200).
    pub dpi: f64,
    /// 0-based pages; `None` = every page.
    pub pages: Option<Vec<usize>>,
    /// Between the file name and the page number: `plan` + `_` + `01` + `.png`.
    pub suffix: String,
    /// Leave markups out.
    pub hide_markups: bool,
}

impl Default for ImageExport {
    fn default() -> Self {
        Self {
            format: ImageFormat::Png,
            dpi: 150.0,
            pages: None,
            suffix: "_".into(),
            hide_markups: false,
        }
    }
}

/// A document export format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficeFormat {
    Text,
    Html,
    Rtf,
    Docx,
    Xlsx,
    Pptx,
}

impl OfficeFormat {
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().trim_start_matches('.') {
            "txt" | "text" => Self::Text,
            "html" | "htm" => Self::Html,
            "rtf" => Self::Rtf,
            "docx" | "word" => Self::Docx,
            "xlsx" | "excel" => Self::Xlsx,
            "pptx" | "powerpoint" => Self::Pptx,
            _ => return None,
        })
    }

    pub fn from_path(p: &Path) -> Option<Self> {
        p.extension().and_then(|e| e.to_str()).and_then(Self::from_name)
    }
}

/// A table read from a page: rows of cells (numbers parsed).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextTable {
    pub rows: Vec<Vec<String>>,
}

impl TextTable {
    pub fn columns(&self) -> usize {
        self.rows.iter().map(Vec::len).max().unwrap_or(0)
    }
}

fn check_pages(s: &Session, pages: &Option<Vec<usize>>) -> Result<Vec<usize>> {
    let list: Vec<usize> = match pages {
        Some(p) => {
            for i in p {
                s.page(*i)?;
            }
            p.clone()
        }
        None => (0..s.page_count()).collect(),
    };
    if list.is_empty() {
        return Err(invalid("no pages to export"));
    }
    if list.len() > MAX_PAGES {
        return Err(invalid(format!("at most {MAX_PAGES} pages can be exported at once")));
    }
    Ok(list)
}

/// Premultiplied RGBA over white as RGB.
fn rgb_on_white(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len() / 4 * 3);
    for p in rgba.as_chunks::<4>().0 {
        let a = 255 - p[3] as u32;
        for c in &p[..3] {
            out.push((*c as u32 + a).min(255) as u8);
        }
    }
    out
}

/// Encode an RGB image.
pub fn encode_rgb(w: usize, h: usize, rgb: Vec<u8>, format: ImageFormat) -> Result<Vec<u8>> {
    let (w32, h32) = (
        u32::try_from(w).map_err(|_| invalid("image too wide"))?,
        u32::try_from(h).map_err(|_| invalid("image too tall"))?,
    );
    if format == ImageFormat::Gif {
        return crate::finish::imaging::gif_encode(w, h, &rgb);
    }
    let img = image::RgbImage::from_raw(w32, h32, rgb).ok_or_else(|| invalid("image size mismatch"))?;
    let mut out = Cursor::new(Vec::new());
    let r = match format {
        ImageFormat::Png => img.write_to(&mut out, image::ImageFormat::Png),
        ImageFormat::Tiff => img.write_to(&mut out, image::ImageFormat::Tiff),
        ImageFormat::Bmp => img.write_to(&mut out, image::ImageFormat::Bmp),
        ImageFormat::Gif => return Err(invalid("GIF is encoded above")),
        ImageFormat::Jpeg(q) => {
            let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, q.clamp(1, 100));
            enc.encode_image(&img)
        }
    };
    r.map_err(|e| invalid(format!("the image could not be encoded: {e}")))?;
    Ok(out.into_inner())
}

/// Lay words out as rows and columns by their positions.
pub fn words_to_table(mut ws: Vec<PageWord>) -> TextTable {
    ws.retain(|w| w.rect.width().is_finite() && w.rect.height().is_finite());
    if ws.is_empty() {
        return TextTable::default();
    }
    let mut heights: Vec<f64> = ws.iter().map(|w| w.rect.height().max(1.0)).collect();
    heights.sort_by(f64::total_cmp);
    let unit = heights.get(heights.len() / 2).copied().unwrap_or(10.0).max(1.0);
    // Lines: top to bottom by the words' centres.
    ws.sort_by(|a, b| (b.rect.y0 + b.rect.y1).total_cmp(&(a.rect.y0 + a.rect.y1)));
    let mut lines: Vec<(f64, Vec<PageWord>)> = Vec::new();
    for w in ws {
        let c = (w.rect.y0 + w.rect.y1) / 2.0;
        match lines.last_mut() {
            Some((lc, l)) if (*lc - c).abs() < unit * 0.5 => l.push(w),
            _ => lines.push((c, vec![w])),
        }
    }
    // Cells: words closer than a gap of about one character height join.
    let mut cells: Vec<Vec<(f64, String)>> = Vec::new();
    for (_, mut l) in lines {
        l.sort_by(|a, b| a.rect.x0.total_cmp(&b.rect.x0));
        let mut row: Vec<(f64, f64, String)> = Vec::new();
        for w in l {
            match row.last_mut() {
                Some((_, x1, t)) if w.rect.x0 - *x1 < unit * 0.9 => {
                    t.push(' ');
                    t.push_str(&w.text);
                    *x1 = w.rect.x1;
                }
                _ => row.push((w.rect.x0, w.rect.x1, w.text.clone())),
            }
        }
        cells.push(row.into_iter().map(|(x0, _, t)| (x0, t)).collect());
    }
    // Columns: cluster the cells' left edges.
    let mut starts: Vec<f64> = cells.iter().flatten().map(|c| c.0).collect();
    starts.sort_by(f64::total_cmp);
    let mut cols: Vec<f64> = Vec::new();
    for x in starts {
        match cols.last() {
            Some(c) if x - *c < unit * 1.5 => {}
            _ => cols.push(x),
        }
    }
    let col_of = |x: f64| cols.iter().rposition(|c| *c <= x + unit * 0.75).unwrap_or(0);
    let rows = cells
        .into_iter()
        .map(|row| {
            let mut out = vec![String::new(); cols.len()];
            for (x, t) in row {
                if let Some(slot) = out.get_mut(col_of(x)) {
                    if !slot.is_empty() {
                        slot.push(' ');
                    }
                    slot.push_str(&t);
                }
            }
            while out.last().is_some_and(String::is_empty) {
                out.pop();
            }
            out
        })
        .collect();
    TextTable { rows }
}

fn xml_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {}
            c => o.push(c),
        }
    }
    o
}

fn col_name(mut i: usize) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b'A' + (i % 26) as u8);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

/// A cell's number, when the whole cell is one (thousands separators allowed).
pub fn cell_number(t: &str) -> Option<f64> {
    let t = t.trim();
    if t.is_empty()
        || !t
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || c == '-' || c == '.')
    {
        return None;
    }
    let clean: String = t.chars().filter(|c| *c != ',').collect();
    clean.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Sheet names: 31 characters, none of `[]:*?/\`.
fn sheet_name(s: &str, i: usize) -> String {
    let n: String = s.chars().filter(|c| !"[]:*?/\\".contains(*c)).take(31).collect();
    if n.trim().is_empty() {
        format!("Sheet{}", i + 1)
    } else {
        n
    }
}

/// An Excel workbook of these sheets.
pub fn xlsx(sheets: &[(String, TextTable)]) -> Vec<u8> {
    let mut zip = pdfcraft_export::Zip::default();
    let mut parts: Vec<(String, String)> = Vec::new();
    let mut ct = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>",
    );
    let mut wb = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets>",
    );
    let mut rels = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    );
    let empty = (String::from("Sheet1"), TextTable::default());
    let list: Vec<&(String, TextTable)> = if sheets.is_empty() {
        vec![&empty]
    } else {
        sheets.iter().collect()
    };
    let mut used: Vec<String> = Vec::new();
    for (i, (name, table)) in list.iter().enumerate() {
        let mut nm = sheet_name(name, i);
        if used.contains(&nm) {
            nm = format!("{} ({})", nm.chars().take(25).collect::<String>(), i + 1);
        }
        used.push(nm.clone());
        let k = i + 1;
        ct.push_str(&format!("<Override PartName=\"/xl/worksheets/sheet{k}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"));
        wb.push_str(&format!(
            "<sheet name=\"{}\" sheetId=\"{k}\" r:id=\"rId{k}\"/>",
            xml_escape(&nm)
        ));
        rels.push_str(&format!("<Relationship Id=\"rId{k}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{k}.xml\"/>"));
        let mut sh = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>",
        );
        for (r, row) in table.rows.iter().enumerate() {
            sh.push_str(&format!("<row r=\"{}\">", r + 1));
            for (c, cell) in row.iter().enumerate() {
                if cell.is_empty() {
                    continue;
                }
                let at = format!("{}{}", col_name(c), r + 1);
                match cell_number(cell) {
                    Some(v) => sh.push_str(&format!("<c r=\"{at}\"><v>{v}</v></c>")),
                    None => sh.push_str(&format!(
                        "<c r=\"{at}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
                        xml_escape(cell)
                    )),
                }
            }
            sh.push_str("</row>");
        }
        sh.push_str("</sheetData></worksheet>");
        parts.push((format!("xl/worksheets/sheet{k}.xml"), sh));
    }
    ct.push_str("</Types>");
    wb.push_str("</sheets></workbook>");
    rels.push_str("</Relationships>");
    zip.add("[Content_Types].xml", ct.as_bytes(), true);
    zip.add(
        "_rels/.rels",
        b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>",
        true,
    );
    zip.add("xl/workbook.xml", wb.as_bytes(), true);
    zip.add("xl/_rels/workbook.xml.rels", rels.as_bytes(), true);
    for (name, body) in &parts {
        zip.add(name, body.as_bytes(), true);
    }
    zip.finish()
}

/// EMUs per point (PowerPoint's unit).
const EMU: f64 = 12_700.0;

/// A PowerPoint deck: one slide per picture (PNG bytes, width and height in points). Slides
/// take the first picture's size.
pub fn pptx(pictures: &[(Vec<u8>, f64, f64)]) -> Vec<u8> {
    let mut zip = pdfcraft_export::Zip::default();
    let (w, h) = pictures.first().map(|p| (p.1, p.2)).unwrap_or((792.0, 612.0));
    // PowerPoint's slide size range: 1 to 56 inches.
    let clamp = |v: f64| (v * EMU).clamp(914_400.0, 51_206_400.0).round() as i64;
    let (cx, cy) = (clamp(w), clamp(h));
    let mut ct = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/><Override PartName=\"/ppt/slideMasters/slideMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml\"/><Override PartName=\"/ppt/slideLayouts/slideLayout1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/><Override PartName=\"/ppt/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>",
    );
    let mut pres_rels = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster\" Target=\"slideMasters/slideMaster1.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme\" Target=\"theme/theme1.xml\"/>",
    );
    let mut ids = String::new();
    let mut parts: Vec<(String, Vec<u8>, bool)> = Vec::new();
    const NS: &str = "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"";
    const TREE: &str = "<p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>";
    for (i, (png, pw, ph)) in pictures.iter().enumerate() {
        let k = i + 1;
        // Fit the picture in the slide, centred.
        let s = (cx as f64 / (pw * EMU)).min(cy as f64 / (ph * EMU));
        let (iw, ih) = ((pw * EMU * s).round() as i64, (ph * EMU * s).round() as i64);
        let (ox, oy) = ((cx - iw) / 2, (cy - ih) / 2);
        ct.push_str(&format!("<Override PartName=\"/ppt/slides/slide{k}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"));
        pres_rels.push_str(&format!("<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"slides/slide{k}.xml\"/>", k + 2));
        ids.push_str(&format!("<p:sldId id=\"{}\" r:id=\"rId{}\"/>", 255 + k, k + 2));
        let slide = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:sld {NS}><p:cSld><p:spTree>{TREE}<p:pic><p:nvPicPr><p:cNvPr id=\"2\" name=\"Page {k}\"/><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"rId2\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x=\"{ox}\" y=\"{oy}\"/><a:ext cx=\"{iw}\" cy=\"{ih}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr></p:pic></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"
        );
        parts.push((format!("ppt/slides/slide{k}.xml"), slide.into_bytes(), true));
        parts.push((
            format!("ppt/slides/_rels/slide{k}.xml.rels"),
            format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout\" Target=\"../slideLayouts/slideLayout1.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"../media/page{k}.png\"/></Relationships>").into_bytes(),
            true,
        ));
        parts.push((format!("ppt/media/page{k}.png"), png.clone(), false));
    }
    ct.push_str("</Types>");
    pres_rels.push_str("</Relationships>");
    let pres = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:presentation {NS}><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst><p:sldIdLst>{ids}</p:sldIdLst><p:sldSz cx=\"{cx}\" cy=\"{cy}\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>"
    );
    let master = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:sldMaster {NS}><p:cSld><p:spTree>{TREE}</p:spTree></p:cSld><p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/><p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst></p:sldMaster>"
    );
    let layout = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:sldLayout {NS} type=\"blank\" preserve=\"1\"><p:cSld name=\"Blank\"><p:spTree>{TREE}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"
    );
    let rel = |t: &str, target: &str| {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/{t}\" Target=\"{target}\"/>{}</Relationships>",
            if t == "slideLayout" {
                "<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme\" Target=\"../theme/theme1.xml\"/>"
            } else {
                ""
            }
        )
    };
    zip.add("[Content_Types].xml", ct.as_bytes(), true);
    zip.add(
        "_rels/.rels",
        b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"ppt/presentation.xml\"/></Relationships>",
        true,
    );
    zip.add("ppt/presentation.xml", pres.as_bytes(), true);
    zip.add("ppt/_rels/presentation.xml.rels", pres_rels.as_bytes(), true);
    zip.add("ppt/slideMasters/slideMaster1.xml", master.as_bytes(), true);
    zip.add(
        "ppt/slideMasters/_rels/slideMaster1.xml.rels",
        rel("slideLayout", "../slideLayouts/slideLayout1.xml").as_bytes(),
        true,
    );
    zip.add("ppt/slideLayouts/slideLayout1.xml", layout.as_bytes(), true);
    zip.add(
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
        rel("slideMaster", "../slideMasters/slideMaster1.xml").as_bytes(),
        true,
    );
    zip.add("ppt/theme/theme1.xml", THEME.as_bytes(), true);
    for (name, body, deflate) in &parts {
        zip.add(name, body, *deflate);
    }
    zip.finish()
}

/// A minimal Office theme (colours, fonts, formats), our own.
const THEME: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"MarkupCraft\"><a:themeElements><a:clrScheme name=\"MarkupCraft\"><a:dk1><a:srgbClr val=\"000000\"/></a:dk1><a:lt1><a:srgbClr val=\"FFFFFF\"/></a:lt1><a:dk2><a:srgbClr val=\"1F2A36\"/></a:dk2><a:lt2><a:srgbClr val=\"E8ECF0\"/></a:lt2><a:accent1><a:srgbClr val=\"2F6FB0\"/></a:accent1><a:accent2><a:srgbClr val=\"C0504D\"/></a:accent2><a:accent3><a:srgbClr val=\"5B9A3C\"/></a:accent3><a:accent4><a:srgbClr val=\"7A5BA6\"/></a:accent4><a:accent5><a:srgbClr val=\"2AA0B0\"/></a:accent5><a:accent6><a:srgbClr val=\"E08A2C\"/></a:accent6><a:hlink><a:srgbClr val=\"0563C1\"/></a:hlink><a:folHlink><a:srgbClr val=\"954F72\"/></a:folHlink></a:clrScheme><a:fontScheme name=\"MarkupCraft\"><a:majorFont><a:latin typeface=\"Arial\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:majorFont><a:minorFont><a:latin typeface=\"Arial\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:minorFont></a:fontScheme><a:fmtScheme name=\"MarkupCraft\"><a:fillStyleLst><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:ln><a:ln w=\"25400\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:ln><a:ln w=\"38100\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>";

impl Session {
    /// Save pages as image files `<dir>/<stem><suffix><n>.<ext>` (numbers zero-padded to the
    /// page count's width). Returns the files written.
    pub fn export_images(&self, dir: &Path, stem: &str, o: &ImageExport) -> Result<Vec<PathBuf>> {
        if !(o.dpi.is_finite() && (18.0..=1200.0).contains(&o.dpi)) {
            return Err(invalid("dpi must be from 18 to 1200"));
        }
        if o.suffix.chars().any(|c| "/\\:*?\"<>|".contains(c) || c.is_control()) {
            return Err(invalid("the suffix cannot hold path or wildcard characters"));
        }
        if !dir.is_dir() {
            return Err(invalid(format!("{} is not a folder", dir.display())));
        }
        let pages = check_pages(self, &o.pages)?;
        let stem = if stem.trim().is_empty() { "page" } else { stem };
        let width = self.page_count().to_string().len();
        let doc = self.renderable(o.hide_markups)?;
        let mut out = Vec::new();
        for p in pages {
            let img = doc.render_rgba(p, (o.dpi / 72.0) as f32)?;
            let bytes = encode_rgb(img.w, img.h, rgb_on_white(&img.rgba), o.format)?;
            let path = dir.join(format!("{stem}{}{:0width$}.{}", o.suffix, p + 1, o.format.extension()));
            write_atomic(&path, &bytes)?;
            out.push(path);
        }
        Ok(out)
    }

    /// The words on a page, in user space.
    pub fn page_words(&self, page: usize) -> Result<Vec<PageWord>> {
        self.page(page)?;
        let doc = Renderable::new(self.current_bytes()?, true)?;
        Ok(match doc.text(page) {
            Some(t) => words(&t, doc.geom(page)?),
            None => Vec::new(),
        })
    }

    /// The words inside `rect` on `page` in reading order (the Select Text tool's selection).
    pub fn text_in_rect(&self, page: usize, rect: Rect) -> Result<String> {
        let t = self.region_table(page, rect)?;
        Ok(t.rows
            .iter()
            .map(|r| {
                r.iter()
                    .filter(|c| !c.is_empty())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" "))
    }

    /// A box of a page as RGBA (markups included), its longer side about `max_px` pixels
    /// (search result thumbnails, snapshots).
    pub fn region_rgba(&self, page: usize, rect: Rect, max_px: u32) -> Result<(usize, usize, Vec<u8>)> {
        let r = rect.normalized();
        if !(r.width() > 0.5 && r.height() > 0.5) {
            return Err(invalid("the box is empty"));
        }
        let scale = (f64::from(max_px.clamp(8, 4096)) / r.width().max(r.height())).clamp(0.05, 8.0) as f32;
        let doc = self.renderable(false)?;
        let img = doc.render_rgba(page, scale)?;
        let s = img.scale as f64;
        let a = img.geom.user_to_view(r.x0 as f32, r.y0 as f32);
        let b = img.geom.user_to_view(r.x1 as f32, r.y1 as f32);
        let x0 = ((a[0].min(b[0]) as f64 * s).floor().max(0.0) as usize).min(img.w);
        let x1 = ((a[0].max(b[0]) as f64 * s).ceil().max(0.0) as usize).min(img.w);
        let y0 = ((a[1].min(b[1]) as f64 * s).floor().max(0.0) as usize).min(img.h);
        let y1 = ((a[1].max(b[1]) as f64 * s).ceil().max(0.0) as usize).min(img.h);
        let (w, h) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
        if w == 0 || h == 0 {
            return Err(invalid("the box is off the page"));
        }
        let mut out = Vec::with_capacity(w * h * 4);
        for y in y0..y1 {
            for x in x0..x1 {
                let i = (y * img.w + x) * 4;
                let p = img.rgba.get(i..i + 4).unwrap_or(&[0, 0, 0, 0]);
                let wh = 255 - p[3] as u32;
                out.extend_from_slice(&[
                    (p[0] as u32 + wh).min(255) as u8,
                    (p[1] as u32 + wh).min(255) as u8,
                    (p[2] as u32 + wh).min(255) as u8,
                    255,
                ]);
            }
        }
        Ok((w, h, out))
    }

    /// [`Self::region_rgba`] as a PNG.
    pub fn region_png(&self, page: usize, rect: Rect, max_px: u32) -> Result<Vec<u8>> {
        let (w, h, rgba) = self.region_rgba(page, rect, max_px)?;
        let rgb: Vec<u8> = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        encode_rgb(w, h, rgb, ImageFormat::Png)
    }

    /// The text inside `rect` on `page` as a table (rows and columns by position).
    pub fn region_table(&self, page: usize, rect: Rect) -> Result<TextTable> {
        let r = rect.normalized();
        if !(r.width() > 1.0 && r.height() > 1.0) {
            return Err(invalid("drag a box around the text to export"));
        }
        let ws: Vec<PageWord> = self
            .page_words(page)?
            .into_iter()
            .filter(|w| {
                let (cx, cy) = ((w.rect.x0 + w.rect.x1) / 2.0, (w.rect.y0 + w.rect.y1) / 2.0);
                cx >= r.x0 && cx <= r.x1 && cy >= r.y0 && cy <= r.y1
            })
            .collect();
        Ok(words_to_table(ws))
    }

    /// Export the text inside `rect` on `page` to `out`: Excel (.xlsx) or CSV (.csv).
    pub fn export_region(&self, page: usize, rect: Rect, out: &Path) -> Result<TextTable> {
        let t = self.region_table(page, rect)?;
        if t.rows.is_empty() {
            return Err(invalid("there is no text in the box"));
        }
        let ext = out
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let bytes = match ext.as_str() {
            "xlsx" => xlsx(&[(format!("Page {}", page + 1), t.clone())]),
            "csv" => {
                let mut s = String::new();
                for row in &t.rows {
                    let cells: Vec<String> = row
                        .iter()
                        .map(|c| {
                            if c.contains([',', '"', '\n']) {
                                format!("\"{}\"", c.replace('"', "\"\""))
                            } else {
                                c.clone()
                            }
                        })
                        .collect();
                    s.push_str(&cells.join(","));
                    s.push_str("\r\n");
                }
                s.into_bytes()
            }
            _ => return Err(invalid("a region exports to .xlsx or .csv")),
        };
        write_atomic(out, &bytes)?;
        Ok(t)
    }

    /// The pages as paragraphs and images for the document writers.
    fn export_pages(&self, pages: &[usize]) -> Result<Vec<pdfcraft_export::Page>> {
        let cos = self.current_copy();
        let mut out = Vec::new();
        for &i in pages {
            let info = self.page(i)?;
            let blocks = pdfcraft_edit::text_blocks(&cos, i)
                .unwrap_or_default()
                .into_iter()
                .filter(|b| !b.text.trim().is_empty())
                .map(|b| {
                    let f = b.base_font.to_ascii_lowercase();
                    pdfcraft_export::Block {
                        text: b.text,
                        rect: b.rect,
                        size: b.size,
                        bold: f.contains("bold") || f.contains("black") || f.contains("heavy"),
                        italic: f.contains("italic") || f.contains("oblique"),
                    }
                })
                .collect();
            let c = info.crop.normalized();
            out.push(pdfcraft_export::Page {
                width: c.width(),
                height: c.height(),
                blocks,
                images: Vec::new(),
            });
        }
        Ok(out)
    }

    /// Export the document (or `pages`) to `out` as text, HTML, RTF, Word, Excel or
    /// PowerPoint (`format`, or the extension of `out`). Returns the bytes written.
    pub fn export_document(
        &self,
        out: &Path,
        format: Option<OfficeFormat>,
        pages: Option<Vec<usize>>,
    ) -> Result<usize> {
        let format = format
            .or_else(|| OfficeFormat::from_path(out))
            .ok_or_else(|| invalid("export to .txt, .html, .rtf, .docx, .xlsx or .pptx"))?;
        let pages = check_pages(self, &pages)?;
        let title = self
            .path()
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Document".into());
        let bytes = match format {
            OfficeFormat::Text => {
                let mut s = String::new();
                for (k, p) in pages.iter().enumerate() {
                    if k > 0 {
                        s.push_str("\r\n\x0c\r\n");
                    }
                    s.push_str(&self.page_text(*p)?.replace('\n', "\r\n"));
                }
                s.into_bytes()
            }
            OfficeFormat::Html => pdfcraft_export::html(&self.export_pages(&pages)?, &title).into_bytes(),
            OfficeFormat::Rtf => pdfcraft_export::rtf(&self.export_pages(&pages)?).into_bytes(),
            OfficeFormat::Docx => pdfcraft_export::docx(&self.export_pages(&pages)?, &title),
            OfficeFormat::Xlsx => {
                let doc = Renderable::new(self.current_bytes()?, true)?;
                let mut sheets = Vec::new();
                for p in &pages {
                    let ws = match doc.text(*p) {
                        Some(t) => words(&t, doc.geom(*p)?),
                        None => Vec::new(),
                    };
                    let label = self.page(*p)?.label.clone();
                    let name = if label.trim().is_empty() {
                        format!("Page {}", p + 1)
                    } else {
                        label
                    };
                    sheets.push((name, words_to_table(ws)));
                }
                xlsx(&sheets)
            }
            OfficeFormat::Pptx => {
                let doc = self.renderable(false)?;
                let mut pics = Vec::new();
                for p in &pages {
                    let geom = doc.geom(*p)?;
                    let (w, h) = (geom.width as f64, geom.height as f64);
                    let scale = (1600.0 / w.max(h).max(1.0)).min(150.0 / 72.0) as f32;
                    let img = doc.render_rgba(*p, scale)?;
                    let png = encode_rgb(img.w, img.h, rgb_on_white(&img.rgba), ImageFormat::Png)?;
                    pics.push((png, w, h));
                }
                pptx(&pics)
            }
        };
        write_atomic(out, &bytes)?;
        Ok(bytes.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, text};

    fn schedule() -> Vec<u8> {
        let mut c = String::new();
        c.push_str(&text(72.0, 700.0, 12.0, "MECHANICAL SCHEDULE"));
        let rows = [
            ("TAG", "CFM", "MODEL"),
            ("AHU-1", "1,200", "XR40"),
            ("AHU-2", "950", "XR30"),
        ];
        for (i, (a, b, m)) in rows.iter().enumerate() {
            let y = 650.0 - 20.0 * i as f64;
            c.push_str(&text(72.0, y, 10.0, a));
            c.push_str(&text(200.0, y, 10.0, b));
            c.push_str(&text(320.0, y, 10.0, m));
        }
        pdf(&[
            SyntheticPage::new(612.0, 792.0, c),
            SyntheticPage::new(612.0, 792.0, text(72.0, 700.0, 18.0, "SECOND")),
        ])
    }

    #[test]
    fn region_reads_a_schedule_as_rows_and_columns() {
        let s = Session::from_bytes(schedule(), "s.pdf").unwrap();
        let t = s.region_table(0, Rect::new(60.0, 600.0, 400.0, 665.0)).unwrap();
        assert_eq!(
            t.rows,
            vec![
                vec!["TAG".to_string(), "CFM".into(), "MODEL".into()],
                vec!["AHU-1".to_string(), "1,200".into(), "XR40".into()],
                vec!["AHU-2".to_string(), "950".into(), "XR30".into()],
            ]
        );
        assert_eq!(cell_number("1,200"), Some(1200.0));
        assert_eq!(cell_number("AHU-1"), None);
        assert!(s.region_table(0, Rect::new(0.0, 0.0, 0.5, 0.5)).is_err());
    }

    #[test]
    fn workbook_and_deck_are_zip_packages() {
        let t = TextTable {
            rows: vec![vec!["A".into(), "2".into()]],
        };
        let x = xlsx(&[("Page 1".into(), t.clone()), ("Page 1".into(), t)]);
        assert_eq!(&x[..2], b"PK");
        let p = pptx(&[(vec![0x89, b'P', b'N', b'G'], 612.0, 792.0)]);
        assert_eq!(&p[..2], b"PK");
        assert_eq!(col_name(0), "A");
        assert_eq!(col_name(27), "AB");
        assert_eq!(sheet_name("a/b:c", 0), "abc");
    }
}
