use eframe::egui::{Color32, ColorImage};
use qrcodegen::{DataTooLong, QrCode, QrCodeEcc};
use thiserror::Error;

/// QR error correction levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QrEcc {
    Low,
    #[default]
    Medium,
    Quartile,
    High,
}

impl From<QrEcc> for QrCodeEcc {
    fn from(ecc: QrEcc) -> Self {
        match ecc {
            QrEcc::Low => Self::Low,
            QrEcc::Medium => Self::Medium,
            QrEcc::Quartile => Self::Quartile,
            QrEcc::High => Self::High,
        }
    }
}

#[derive(Debug, Error)]
pub enum QrError {
    #[error("data exceeds maximum QR code capacity: {0}")]
    CapacityExceeded(#[from] DataTooLong),
}

/// 2D boolean grid representing a generated QR symbol with quiet zone borders.
#[derive(Debug, Clone)]
pub struct QrMatrix {
    pub size: usize,
    modules: Vec<bool>,
}

impl QrMatrix {
    #[must_use]
    pub fn get(&self, x: usize, y: usize) -> bool {
        if x < self.size && y < self.size {
            self.modules[y * self.size + x]
        } else {
            false
        }
    }
}

/// Generates a QR code matrix including the standard 4-module quiet zone.
///
/// Complexity: O(N^2) where N is the symbol version module dimension.
pub fn encode_text(text: &str, ecc: QrEcc) -> Result<QrMatrix, QrError> {
    let qr = QrCode::encode_text(text, ecc.into())?;
    let raw_size = qr.size();
    let quiet_zone = 4_i32;
    let total_size = usize::try_from((raw_size + quiet_zone * 2).max(0)).unwrap_or_default();
    let mut modules = vec![false; total_size * total_size];

    for y in 0..raw_size {
        for x in 0..raw_size {
            if qr.get_module(x, y) {
                let module_x = usize::try_from(x + quiet_zone).unwrap_or_default();
                let module_y = usize::try_from(y + quiet_zone).unwrap_or_default();
                modules[module_y * total_size + module_x] = true;
            }
        }
    }

    Ok(QrMatrix {
        size: total_size,
        modules,
    })
}

/// Renders a QR matrix centered on a grayscale canvas of `target_width` x `target_height`.
///
/// Complexity: O(W * H) where W is `target_width` and H is `target_height`.
#[must_use]
pub fn render_to_gray_frame(
    matrix: &QrMatrix,
    target_width: usize,
    target_height: usize,
) -> Vec<u8> {
    let mut buffer = vec![255_u8; target_width * target_height];
    if matrix.size == 0 || target_width == 0 || target_height == 0 {
        return buffer;
    }

    let scale = (target_width / matrix.size)
        .min(target_height / matrix.size)
        .max(1);
    let qr_pixels = matrix.size * scale;
    let offset_x = (target_width.saturating_sub(qr_pixels)) / 2;
    let offset_y = (target_height.saturating_sub(qr_pixels)) / 2;

    for y in 0..matrix.size {
        for x in 0..matrix.size {
            if matrix.get(x, y) {
                let origin_x = offset_x + x * scale;
                let origin_y = offset_y + y * scale;
                for row in 0..scale {
                    let pixel_y = origin_y + row;
                    if pixel_y >= target_height {
                        break;
                    }
                    let row_offset = pixel_y * target_width;
                    for col in 0..scale {
                        let pixel_x = origin_x + col;
                        if pixel_x < target_width {
                            buffer[row_offset + pixel_x] = 0;
                        }
                    }
                }
            }
        }
    }

    buffer
}

/// Renders a QR matrix directly into an `egui::ColorImage` for display in the UI.
///
/// Complexity: O(S^2 * M^2) where M is `matrix.size` and S is `pixel_scale`.
#[must_use]
pub fn render_to_color_image(matrix: &QrMatrix, pixel_scale: usize) -> ColorImage {
    let scale = pixel_scale.max(1);
    let image_dimension = matrix.size * scale;
    let mut pixels = vec![Color32::WHITE; image_dimension * image_dimension];

    for y in 0..matrix.size {
        for x in 0..matrix.size {
            if matrix.get(x, y) {
                let start_x = x * scale;
                let start_y = y * scale;
                for row in 0..scale {
                    let py = start_y + row;
                    let row_offset = py * image_dimension;
                    for col in 0..scale {
                        let px = start_x + col;
                        pixels[row_offset + px] = Color32::BLACK;
                    }
                }
            }
        }
    }

    ColorImage::new([image_dimension, image_dimension], pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_renders_qr() {
        let text = "FQ1|test|abcd|1|1|4|1234";
        let matrix = encode_text(text, QrEcc::Medium).expect("encode text");
        assert!(matrix.size > 8);
        assert!(!matrix.get(0, 0), "quiet zone module must be white");

        let gray = render_to_gray_frame(&matrix, 200, 200);
        assert_eq!(gray.len(), 40_000);
        assert!(gray.contains(&0), "canvas must contain black pixels");
        assert!(gray.contains(&255), "canvas must contain white pixels");

        let color = render_to_color_image(&matrix, 2);
        assert_eq!(color.size, [matrix.size * 2, matrix.size * 2]);
    }
}
