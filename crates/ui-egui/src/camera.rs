//! The system camera (the `camera` feature): `nokhwa` on the operating system's camera API
//! (Media Foundation, AVFoundation, Video4Linux). A camera is opened for each picture and
//! closed again, so nothing keeps it busy between pictures.

use markupcraft_engine::devices::{Camera, Frame};
use markupcraft_engine::{EngineError, Result};
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{ApiBackend, CameraIndex, RequestedFormat, RequestedFormatType};

/// The cameras the operating system lists.
pub struct SystemCamera;

impl SystemCamera {
    /// `None` when the platform has no camera API nokhwa can use.
    pub fn new() -> Option<Self> {
        #[cfg(target_os = "macos")]
        nokhwa::nokhwa_initialize(|_| {});
        Some(Self)
    }
}

fn err(e: impl std::fmt::Display) -> EngineError {
    EngineError::Invalid(format!("camera: {e}"))
}

impl Camera for SystemCamera {
    fn devices(&mut self) -> Vec<String> {
        nokhwa::query(ApiBackend::Auto)
            .map(|v| v.into_iter().take(32).map(|c| c.human_name()).collect())
            .unwrap_or_default()
    }

    fn capture(&mut self, index: usize) -> Result<Frame> {
        let i = u32::try_from(index).map_err(err)?;
        let want = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestResolution);
        let mut cam = nokhwa::Camera::new(CameraIndex::Index(i), want).map_err(err)?;
        cam.open_stream().map_err(err)?;
        // The first frames of many cameras are dark while exposure settles.
        let mut last = None;
        for _ in 0..5 {
            last = Some(cam.frame().map_err(err)?);
        }
        let _ = cam.stop_stream();
        let buf = last.ok_or_else(|| err("no picture"))?;
        let img = buf.decode_image::<RgbFormat>().map_err(err)?;
        Ok(Frame {
            width: img.width(),
            height: img.height(),
            rgb: img.into_raw(),
        })
    }
}
