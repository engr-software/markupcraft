//! Print's Advanced options, applied to the print-ready PDF before it goes to the printer:
//! print in grayscale, and print as image (each sheet rasterized at a chosen resolution, for
//! printers that struggle with complex vector drawings).

use std::path::Path;
use std::sync::Arc;

use super::imaging::{Picture, pictures_pdf};
use crate::archive::ColorMode;
use crate::raster::Renderable;
use crate::{Result, Session, invalid};

/// The Advanced print options.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PrintAdvanced {
    pub grayscale: bool,
    /// Print as image at this resolution (72 to 600 dpi).
    pub as_image: Option<f64>,
}

/// Apply `a` to the print-ready PDF at `path` (rewritten in place).
pub fn post_process(path: &Path, a: &PrintAdvanced) -> Result<()> {
    if a == &PrintAdvanced::default() {
        return Ok(());
    }
    if let Some(dpi) = a.as_image
        && !(dpi.is_finite() && (72.0..=600.0).contains(&dpi))
    {
        return Err(invalid("print as image: 72 to 600 dpi"));
    }
    let mut s = Session::open(path)?;
    if a.grayscale {
        s.color_process(&[], ColorMode::Grayscale)?;
    }
    let bytes = s.current_bytes()?;
    let out = match a.as_image {
        Some(dpi) => {
            let doc = Renderable::new(Arc::new(bytes.to_vec()), false)?;
            let mut pics = Vec::with_capacity(doc.page_count());
            for p in 0..doc.page_count() {
                let img = doc.render_rgba(p, (dpi / 72.0) as f32)?;
                let rgb: Vec<u8> = img
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|px| {
                        let a = u16::from(px[3]);
                        // premultiplied colour over white
                        [0usize, 1, 2].map(|i| (u16::from(px[i]) + (255 - a)).min(255) as u8)
                    })
                    .collect();
                let ppi = f64::from(img.scale) * 72.0;
                pics.push(Picture {
                    w: u32::try_from(img.w).map_err(|_| invalid("sheet too wide"))?,
                    h: u32::try_from(img.h).map_err(|_| invalid("sheet too tall"))?,
                    rgb,
                    ppi,
                });
            }
            pictures_pdf(&pics)?
        }
        None => bytes.to_vec(),
    };
    crate::write_atomic(path, &out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticPage, pdf, rect};

    #[test]
    fn grayscale_and_as_image() {
        let p = std::env::temp_dir().join(format!("mc-printadv-{}.pdf", std::process::id()));
        std::fs::write(
            &p,
            pdf(&[SyntheticPage::new(
                612.0,
                792.0,
                format!("1 0 0 rg {}", rect(100.0, 100.0, 200.0, 200.0)),
            )]),
        )
        .unwrap();
        post_process(
            &p,
            &PrintAdvanced {
                grayscale: true,
                as_image: Some(72.0),
            },
        )
        .unwrap();
        let s = Session::open(&p).unwrap();
        assert_eq!(s.page_count(), 1);
        let m = s.doc().pages[0].media.normalized();
        assert!((m.width() - 612.0).abs() < 2.0, "{m:?}");
        let r = Renderable::new(s.current_bytes().unwrap(), false).unwrap();
        let img = r.render_rgba(0, 1.0).unwrap();
        // inside the (formerly red) box the colour is grey now
        let i = ((792 - 200) * img.w + 200) * 4;
        let px = &img.rgba[i..i + 3];
        assert!(px[0].abs_diff(px[1]) < 8 && px[1].abs_diff(px[2]) < 8, "{px:?}");
        assert!(
            post_process(
                &p,
                &PrintAdvanced {
                    grayscale: false,
                    as_image: Some(5.0)
                }
            )
            .is_err()
        );
        let _ = std::fs::remove_file(p);
    }
}
