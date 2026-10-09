//! Scanners and cameras: images acquired from a device become pages (Insert > From Scanner,
//! Create > From Scanner or Camera), image markups (Image From Scanner, Camera) or a new PDF.
//!
//! Scanners are reached over eSCL ("AirScan", the Mopria / Apple network scan protocol: plain
//! HTTP and XML): read the capabilities, post a scan job, fetch each page until the scanner
//! has no more. Only `http://` scanner addresses are supported (no TLS). Cameras are a
//! [`Camera`] the application supplies (the desktop app's comes from the operating system's
//! camera API when built with its `camera` feature); tests use [`TestCamera`].

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

use markupcraft_model::{Point, Rect};

use crate::stamps::{StampPlace, StampSource};
use crate::{EngineError, Result, Session, invalid};

/// Largest HTTP body read (a scanned page).
const MAX_BODY: usize = 128 << 20;
/// Most pages taken from one scan job.
pub const MAX_SCAN_PAGES: usize = 500;
/// Largest header block.
const MAX_HEADERS: usize = 64 << 10;

// ---- a small HTTP/1.1 client (eSCL needs GET, POST and DELETE) ------------------------------

/// An HTTP response.
#[derive(Debug, Clone, PartialEq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// `http://host[:port]/path` split into (host, port, path).
pub fn parse_http_url(url: &str) -> Result<(String, u16, String)> {
    let rest = url
        .trim()
        .strip_prefix("http://")
        .ok_or_else(|| invalid("the scanner address starts with http:// (eSCL over TLS is not supported)"))?;
    if rest.chars().any(|c| c.is_whitespace() || c.is_control()) || rest.len() > 2048 {
        return Err(invalid("the address holds spaces or control characters"));
    }
    let (hostport, path) = match rest.find('/') {
        Some(i) => (rest.get(..i).unwrap_or_default(), rest.get(i..).unwrap_or("/")),
        None => (rest, "/"),
    };
    let (host, port) = if let Some(h) = hostport.strip_prefix('[') {
        // [IPv6]:port
        let (h, p) = h.split_once(']').ok_or_else(|| invalid("a bad IPv6 address"))?;
        (h.to_string(), p.strip_prefix(':').unwrap_or("80"))
    } else {
        match hostport.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p),
            None => (hostport.to_string(), "80"),
        }
    };
    if host.is_empty() {
        return Err(invalid("the address has no host"));
    }
    let port: u16 = port.parse().map_err(|_| invalid(format!("bad port {port:?}")))?;
    Ok((host, port, path.to_string()))
}

/// One HTTP request (`Connection: close`).
pub fn http(method: &str, url: &str, body: Option<(&str, &[u8])>, timeout: Duration) -> Result<HttpResponse> {
    let (host, port, path) = parse_http_url(url)?;
    let io = |e: std::io::Error| EngineError::Io {
        path: url.to_string(),
        source: e,
    };
    let addr = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(io)?
        .next()
        .ok_or_else(|| invalid(format!("{host} has no address")))?;
    let mut s = TcpStream::connect_timeout(&addr, timeout).map_err(io)?;
    s.set_read_timeout(Some(timeout)).map_err(io)?;
    s.set_write_timeout(Some(timeout)).map_err(io)?;
    let host_header = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host_header}\r\nUser-Agent: MarkupCraft\r\nAccept: */*\r\nConnection: close\r\n"
    );
    if let Some((ct, b)) = body {
        req.push_str(&format!("Content-Type: {ct}\r\nContent-Length: {}\r\n", b.len()));
    }
    req.push_str("\r\n");
    // One write: a server that answers after the headers must still see the whole request.
    let mut all = req.into_bytes();
    if let Some((_, b)) = body {
        all.extend_from_slice(b);
    }
    s.write_all(&all).map_err(io)?;
    let mut r = BufReader::new(s);
    let mut status_line = String::new();
    r.read_line(&mut status_line).map_err(io)?;
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| invalid(format!("{url}: not an HTTP response")))?;
    let mut headers = Vec::new();
    let mut seen = 0usize;
    loop {
        let mut line = String::new();
        let n = r.read_line(&mut line).map_err(io)?;
        seen = seen.saturating_add(n);
        if n == 0 || line == "\r\n" || line == "\n" || seen > MAX_HEADERS {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let find = |k: &str| {
        headers
            .iter()
            .find(|(h, _)| h.eq_ignore_ascii_case(k))
            .map(|(_, v)| v.clone())
    };
    let mut body = Vec::new();
    if find("Transfer-Encoding").is_some_and(|v| v.to_ascii_lowercase().contains("chunked")) {
        loop {
            let mut size = String::new();
            r.read_line(&mut size).map_err(io)?;
            let n = usize::from_str_radix(size.trim().split(';').next().unwrap_or("0"), 16)
                .map_err(|_| invalid(format!("{url}: a bad chunk")))?;
            if n == 0 {
                break;
            }
            if body.len().saturating_add(n) > MAX_BODY {
                return Err(invalid(format!("{url}: the response is too large")));
            }
            let start = body.len();
            body.resize(start + n, 0);
            r.read_exact(body.get_mut(start..).unwrap_or_default()).map_err(io)?;
            let mut crlf = [0u8; 2];
            r.read_exact(&mut crlf).map_err(io)?;
        }
    } else if let Some(len) = find("Content-Length").and_then(|v| v.parse::<usize>().ok()) {
        if len > MAX_BODY {
            return Err(invalid(format!("{url}: the response is too large")));
        }
        body.resize(len, 0);
        r.read_exact(&mut body).map_err(io)?;
    } else if method != "HEAD" && status != 204 && status != 304 {
        r.take(MAX_BODY as u64 + 1).read_to_end(&mut body).map_err(io)?;
        if body.len() > MAX_BODY {
            return Err(invalid(format!("{url}: the response is too large")));
        }
    }
    Ok(HttpResponse { status, headers, body })
}

// ---- eSCL ---------------------------------------------------------------------------------

/// The text of every `<tag>` (any namespace prefix) in `xml`.
fn tag_texts(xml: &str, tag: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(o) = xml.get(i..).and_then(|x| x.find('<')).map(|o| o + i) {
        let Some(c) = xml.get(o..).and_then(|x| x.find('>')).map(|c| c + o) else {
            break;
        };
        let name = xml.get(o + 1..c).unwrap_or_default();
        let local = name.rsplit(':').next().unwrap_or(name);
        if local == tag
            && let Some(end) = xml.get(c + 1..).and_then(|x| x.find("</")).map(|e| e + c + 1)
        {
            out.push(xml.get(c + 1..end).unwrap_or_default().trim().to_string());
            i = end;
        } else {
            i = c + 1;
        }
        if out.len() > 1_000 {
            break;
        }
    }
    out
}

/// What a scanner can do (from `ScannerCapabilities`).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize)]
pub struct ScannerCaps {
    pub make_and_model: String,
    /// `Platen`, `Feeder`.
    pub sources: Vec<String>,
    /// Dots per inch.
    pub resolutions: Vec<u32>,
    /// `RGB24`, `Grayscale8`, `BlackAndWhite1`.
    pub color_modes: Vec<String>,
    /// MIME types (`image/jpeg`, `application/pdf`).
    pub formats: Vec<String>,
}

/// A scan job's settings.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanSettings {
    pub source: String,
    pub resolution: u32,
    pub color_mode: String,
    pub format: String,
}

impl Default for ScanSettings {
    fn default() -> Self {
        Self {
            source: "Platen".into(),
            resolution: 300,
            color_mode: "RGB24".into(),
            format: "image/jpeg".into(),
        }
    }
}

/// A network scanner at its eSCL root (`http://192.168.1.20/eSCL`).
#[derive(Debug, Clone, PartialEq)]
pub struct EsclScanner {
    pub base: String,
    pub timeout: Duration,
}

impl EsclScanner {
    pub fn new(base: &str) -> Result<Self> {
        let base = base.trim().trim_end_matches('/').to_string();
        parse_http_url(&base)?;
        Ok(Self {
            base,
            timeout: Duration::from_secs(60),
        })
    }

    pub fn capabilities(&self) -> Result<ScannerCaps> {
        let r = http("GET", &format!("{}/ScannerCapabilities", self.base), None, self.timeout)?;
        if r.status != 200 {
            return Err(invalid(format!(
                "the scanner answered {} to ScannerCapabilities",
                r.status
            )));
        }
        let x = String::from_utf8_lossy(&r.body);
        let mut caps = ScannerCaps {
            make_and_model: tag_texts(&x, "MakeAndModel").into_iter().next().unwrap_or_default(),
            ..Default::default()
        };
        for (tag, src) in [("Platen", "Platen"), ("Adf", "Feeder")] {
            if x.contains(&format!(":{tag}>")) || x.contains(&format!("<{tag}>")) {
                caps.sources.push(src.to_string());
            }
        }
        let mut res: Vec<u32> = tag_texts(&x, "XResolution")
            .iter()
            .filter_map(|v| v.parse().ok())
            .collect();
        res.sort_unstable();
        res.dedup();
        caps.resolutions = res;
        let mut modes = tag_texts(&x, "ColorMode");
        modes.dedup();
        modes.sort();
        modes.dedup();
        caps.color_modes = modes;
        let mut f = tag_texts(&x, "DocumentFormat");
        f.extend(tag_texts(&x, "DocumentFormatExt"));
        f.sort();
        f.dedup();
        caps.formats = f;
        Ok(caps)
    }

    /// Run a scan job: every page the scanner gives, as image (or PDF) bytes.
    pub fn scan(&self, s: &ScanSettings) -> Result<Vec<Vec<u8>>> {
        for (what, v) in [
            ("source", &s.source),
            ("color mode", &s.color_mode),
            ("format", &s.format),
        ] {
            if v.is_empty() || v.len() > 64 || !v.chars().all(|c| c.is_ascii_alphanumeric() || "/-+.".contains(c)) {
                return Err(invalid(format!("a bad scan {what}: {v:?}")));
            }
        }
        if !(50..=4800).contains(&s.resolution) {
            return Err(invalid("the resolution is 50 to 4800 dpi"));
        }
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<scan:ScanSettings xmlns:scan=\"http://schemas.hp.com/imaging/escl/2011/05/03\" xmlns:pwg=\"http://www.pwg.org/schemas/2010/12/sm\">\n<pwg:Version>2.6</pwg:Version>\n<pwg:InputSource>{}</pwg:InputSource>\n<scan:ColorMode>{}</scan:ColorMode>\n<scan:XResolution>{}</scan:XResolution>\n<scan:YResolution>{}</scan:YResolution>\n<pwg:DocumentFormat>{}</pwg:DocumentFormat>\n<scan:DocumentFormatExt>{}</scan:DocumentFormatExt>\n</scan:ScanSettings>\n",
            s.source, s.color_mode, s.resolution, s.resolution, s.format, s.format
        );
        let r = http(
            "POST",
            &format!("{}/ScanJobs", self.base),
            Some(("text/xml", xml.as_bytes())),
            self.timeout,
        )?;
        if r.status != 201 {
            return Err(invalid(format!("the scanner refused the job (HTTP {})", r.status)));
        }
        let loc = r
            .header("Location")
            .ok_or_else(|| invalid("the scanner gave no job address"))?
            .trim()
            .to_string();
        let job = if loc.starts_with("http://") {
            loc
        } else {
            let (host, port, _) = parse_http_url(&self.base)?;
            let host = if host.contains(':') { format!("[{host}]") } else { host };
            format!(
                "http://{host}:{port}{}",
                if loc.starts_with('/') { loc } else { format!("/{loc}") }
            )
        };
        let job = job.trim_end_matches('/').to_string();
        let mut pages = Vec::new();
        let mut busy = 0;
        while pages.len() < MAX_SCAN_PAGES {
            let r = http("GET", &format!("{job}/NextDocument"), None, self.timeout)?;
            match r.status {
                200 => {
                    if r.body.is_empty() {
                        break;
                    }
                    pages.push(r.body);
                    busy = 0;
                    if s.source == "Platen" {
                        break;
                    }
                }
                503 if busy < 20 => {
                    busy += 1;
                    std::thread::sleep(Duration::from_millis(250));
                }
                _ => break,
            }
        }
        // Tidy up on the scanner; failures here do not matter.
        let _ = http("DELETE", &job, None, Duration::from_secs(5));
        if pages.is_empty() {
            return Err(invalid("the scanner returned no pages"));
        }
        Ok(pages)
    }
}

// ---- cameras --------------------------------------------------------------------------------

/// A captured frame (8-bit RGB, row-major).
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

impl Frame {
    /// The frame as PNG bytes.
    pub fn png(&self) -> Result<Vec<u8>> {
        let img = image::RgbImage::from_raw(self.width, self.height, self.rgb.clone())
            .ok_or_else(|| invalid("the frame's size does not match its pixels"))?;
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png)
            .map_err(|e| invalid(e.to_string()))?;
        Ok(out.into_inner())
    }
}

/// A camera the application can take pictures with.
pub trait Camera: Send {
    /// The cameras available, by name.
    fn devices(&mut self) -> Vec<String>;
    /// One picture from camera `index`.
    fn capture(&mut self, index: usize) -> Result<Frame>;
}

/// A camera for tests and headless runs: a test pattern of the given size.
#[derive(Debug, Clone, Copy)]
pub struct TestCamera {
    pub width: u32,
    pub height: u32,
}

impl Camera for TestCamera {
    fn devices(&mut self) -> Vec<String> {
        vec!["Test pattern".into()]
    }

    fn capture(&mut self, index: usize) -> Result<Frame> {
        if index != 0 {
            return Err(invalid(format!("no camera {index}")));
        }
        let (w, h) = (self.width.clamp(1, 4096), self.height.clamp(1, 4096));
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                rgb.extend_from_slice(&[(x * 255 / w) as u8, (y * 255 / h) as u8, 160]);
            }
        }
        Ok(Frame {
            width: w,
            height: h,
            rgb,
        })
    }
}

// ---- acquired images into documents ---------------------------------------------------------------

/// The file extension an acquired image's bytes call for.
fn ext_of(bytes: &[u8]) -> Result<&'static str> {
    if bytes.starts_with(b"%PDF") {
        Ok("pdf")
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        Ok("jpg")
    } else if bytes.starts_with(b"\x89PNG") {
        Ok("png")
    } else if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        Ok("tif")
    } else {
        Err(invalid("the device sent something other than JPEG, PNG, TIFF or PDF"))
    }
}

/// Write acquired images to temporary files (removed when the guard drops).
struct Temps(Vec<PathBuf>);

impl Drop for Temps {
    fn drop(&mut self) {
        for p in &self.0 {
            let _ = std::fs::remove_file(p);
        }
    }
}

fn temps(images: &[Vec<u8>]) -> Result<Temps> {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut t = Temps(Vec::new());
    for b in images {
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!(
            "markupcraft-acquired-{}-{n}.{}",
            std::process::id(),
            ext_of(b)?
        ));
        crate::write_atomic(&p, b)?;
        t.0.push(p);
    }
    Ok(t)
}

/// A new PDF at `out` from acquired images (one page each, 72 ppi up to 17 x 22 in) or PDFs.
/// Returns the page count.
pub fn pdf_from_images(images: &[Vec<u8>], out: &Path) -> Result<usize> {
    if images.is_empty() {
        return Err(invalid("nothing was acquired"));
    }
    let t = temps(images)?;
    crate::docs_more::create_pdf_from_files(&t.0, out)
}

impl Session {
    /// Insert acquired images (or PDFs) as pages so the first becomes page `at`. Undoable.
    pub fn insert_acquired_pages(&mut self, at: usize, images: &[Vec<u8>]) -> Result<usize> {
        let tmp = std::env::temp_dir().join(format!(
            "markupcraft-acquired-{}-{}.pdf",
            std::process::id(),
            crate::stamps::now_secs()
        ));
        let n = pdf_from_images(images, &tmp)?;
        let r = self.insert_file_pages(at, &tmp, None);
        let _ = std::fs::remove_file(&tmp);
        r.map(|_| n)
    }

    /// Place an acquired image as an image markup on `page`, centred at `at` or in a box.
    /// Returns its id. Undoable.
    pub fn add_acquired_image(&mut self, page: usize, at: StampPlace, image: &[u8]) -> Result<String> {
        // A stamp image is a PNG (or a PDF page): re-encode JPEG and TIFF scans.
        let png;
        let image = if matches!(ext_of(image)?, "jpg" | "tif") {
            let img = image::load_from_memory(image).map_err(|e| invalid(e.to_string()))?;
            let mut out = std::io::Cursor::new(Vec::new());
            img.write_to(&mut out, image::ImageFormat::Png)
                .map_err(|e| invalid(e.to_string()))?;
            png = out.into_inner();
            png.as_slice()
        } else {
            image
        };
        let t = temps(&[image.to_vec()])?;
        let p = t.0.first().ok_or_else(|| invalid("nothing was acquired"))?;
        let at = match at {
            // An image placed by its centre: 3 inches wide at most.
            StampPlace::Center(c) => {
                let img = image::load_from_memory(image).map_err(|e| invalid(e.to_string()))?;
                let (w, h) = (f64::from(img.width().max(1)), f64::from(img.height().max(1)));
                let k = (216.0 / w).min(216.0 / h);
                StampPlace::Rect(Rect::new(
                    c.x - w * k / 2.0,
                    c.y - h * k / 2.0,
                    c.x + w * k / 2.0,
                    c.y + h * k / 2.0,
                ))
            }
            r => r,
        };
        self.place_stamp(
            page,
            at,
            &StampSource::File {
                path: p.clone(),
                page: 0,
            },
            None,
            &Default::default(),
            None,
        )
    }
}

/// An acquired image no larger than `max` pixels on its longest side (re-encoded as PNG when
/// it had to shrink; PDFs and smaller images come back as they are).
pub fn limit_image_side(bytes: &[u8], max: u32) -> Result<Vec<u8>> {
    if bytes.starts_with(b"%PDF") {
        return Ok(bytes.to_vec());
    }
    let img = image::load_from_memory(bytes).map_err(|e| invalid(e.to_string()))?;
    let max = max.max(16);
    if img.width().max(img.height()) <= max {
        return Ok(bytes.to_vec());
    }
    let small = img.resize(max, max, image::imageops::FilterType::Triangle);
    let mut out = std::io::Cursor::new(Vec::new());
    small
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| invalid(e.to_string()))?;
    Ok(out.into_inner())
}

/// A point's default image box (the centre of `page`'s crop box).
pub fn page_centre(s: &Session, page: usize) -> Result<Point> {
    let c = s.page(page)?.crop.normalized();
    Ok(Point::new((c.x0 + c.x1) / 2.0, (c.y0 + c.y1) / 2.0))
}

#[cfg(test)]
pub(crate) mod fake {
    //! A fake eSCL scanner on loopback for tests.
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Serve `pages` (JPEG bytes) to one scan job; returns the scanner's eSCL root.
    pub fn scanner(pages: Vec<Vec<u8>>) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            let mut left = pages;
            for s in l.incoming().take(20) {
                let Ok(mut s) = s else { continue };
                let mut buf = vec![0u8; 65536];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let line = req.lines().next().unwrap_or_default().to_string();
                let reply = |s: &mut std::net::TcpStream, status: &str, headers: &str, body: &[u8]| {
                    let _ = s.write_all(
                        format!("HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\n\r\n", body.len()).as_bytes(),
                    );
                    let _ = s.write_all(body);
                };
                if line.starts_with("GET /eSCL/ScannerCapabilities") {
                    let caps = "<?xml version=\"1.0\"?><scan:ScannerCapabilities xmlns:scan=\"x\" xmlns:pwg=\"y\"><pwg:MakeAndModel>Test Scanner 9000</pwg:MakeAndModel><scan:Platen><scan:PlatenInputCaps><scan:SettingProfiles><scan:SettingProfile><scan:ColorModes><scan:ColorMode>RGB24</scan:ColorMode><scan:ColorMode>Grayscale8</scan:ColorMode></scan:ColorModes><scan:DocumentFormats><pwg:DocumentFormat>image/jpeg</pwg:DocumentFormat><pwg:DocumentFormat>application/pdf</pwg:DocumentFormat></scan:DocumentFormats><scan:SupportedResolutions><scan:DiscreteResolutions><scan:DiscreteResolution><scan:XResolution>300</scan:XResolution><scan:YResolution>300</scan:YResolution></scan:DiscreteResolution><scan:DiscreteResolution><scan:XResolution>150</scan:XResolution><scan:YResolution>150</scan:YResolution></scan:DiscreteResolution></scan:DiscreteResolutions></scan:SupportedResolutions></scan:SettingProfile></scan:SettingProfiles></scan:PlatenInputCaps></scan:Platen><scan:Adf></scan:Adf></scan:ScannerCapabilities>";
                    reply(&mut s, "200 OK", "Content-Type: text/xml\r\n", caps.as_bytes());
                } else if line.starts_with("POST /eSCL/ScanJobs") {
                    reply(
                        &mut s,
                        "201 Created",
                        &format!("Location: http://{addr}/eSCL/ScanJobs/7\r\n"),
                        b"",
                    );
                } else if line.starts_with("GET /eSCL/ScanJobs/7/NextDocument") {
                    if left.is_empty() {
                        reply(&mut s, "404 Not Found", "", b"");
                    } else {
                        let p = left.remove(0);
                        reply(&mut s, "200 OK", "Content-Type: image/jpeg\r\n", &p);
                    }
                } else {
                    reply(&mut s, "200 OK", "", b"");
                }
            }
        });
        format!("http://{addr}/eSCL")
    }

    /// A small JPEG.
    pub fn jpeg(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x % 255) as u8, (y % 255) as u8, 90]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
        out.into_inner()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf};

    #[test]
    fn urls_parse_and_bad_ones_are_refused() {
        assert_eq!(
            parse_http_url("http://10.0.0.5/eSCL").unwrap(),
            ("10.0.0.5".into(), 80, "/eSCL".into())
        );
        assert_eq!(
            parse_http_url("http://scan.local:8080").unwrap(),
            ("scan.local".into(), 8080, "/".into())
        );
        assert_eq!(
            parse_http_url("http://[::1]:9/x").unwrap(),
            ("::1".into(), 9, "/x".into())
        );
        assert!(parse_http_url("https://x/eSCL").is_err());
        assert!(parse_http_url("http://x y/").is_err());
        assert!(parse_http_url("http://:80/").is_err());
    }

    #[test]
    fn an_escl_scanner_scans_pages_into_a_document() {
        let base = fake::scanner(vec![fake::jpeg(120, 160), fake::jpeg(100, 100)]);
        let sc = EsclScanner::new(&base).unwrap();
        let caps = sc.capabilities().unwrap();
        assert_eq!(caps.make_and_model, "Test Scanner 9000");
        assert_eq!(caps.sources, ["Platen", "Feeder"]);
        assert_eq!(caps.resolutions, [150, 300]);
        assert!(caps.formats.contains(&"image/jpeg".to_string()));
        let pages = sc
            .scan(&ScanSettings {
                source: "Feeder".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(pages.len(), 2);
        let p = std::env::temp_dir().join(format!("markupcraft-scan-{}.pdf", std::process::id()));
        let mut s = Session::from_bytes(pdf(&[SyntheticPage::new(612.0, 792.0, "")]), &p).unwrap();
        assert_eq!(s.insert_acquired_pages(1, &pages).unwrap(), 2);
        assert_eq!(s.page_count(), 3);
        assert!((s.page(1).unwrap().media.width() - 120.0).abs() < 1.0);
        // As an image markup.
        let id = s
            .add_acquired_image(0, StampPlace::Center(Point::new(300.0, 400.0)), pages.first().unwrap())
            .unwrap();
        assert_eq!(s.markup(&id).unwrap().kind, markupcraft_model::Kind::Stamp);
        assert!(
            sc.scan(&ScanSettings {
                resolution: 1,
                ..Default::default()
            })
            .is_err()
        );
    }

    #[test]
    fn a_camera_frame_becomes_a_pdf() {
        let mut cam = TestCamera { width: 64, height: 48 };
        assert_eq!(cam.devices(), ["Test pattern"]);
        let f = cam.capture(0).unwrap();
        assert!(cam.capture(1).is_err());
        let png = f.png().unwrap();
        let out = std::env::temp_dir().join(format!("markupcraft-camera-{}.pdf", std::process::id()));
        assert_eq!(pdf_from_images(&[png], &out).unwrap(), 1);
        assert!(ext_of(b"GIF89a").is_err());
        // Pictures larger than the capture limit shrink to it.
        let small = limit_image_side(&f.png().unwrap(), 32).unwrap();
        let img = image::load_from_memory(&small).unwrap();
        assert_eq!((img.width(), img.height()), (32, 24));
    }
}
