use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};

/// Remove fully transparent rows and columns from all edges of an image.
pub fn remove_transparent_padding(img: &DynamicImage) -> DynamicImage {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();

    if w == 0 || h == 0 {
        return img.clone();
    }

    // Find top bound
    let mut top = 0u32;
    'top: for y in 0..h {
        for x in 0..w {
            if rgba.get_pixel(x, y)[3] > 0 {
                break 'top;
            }
        }
        top = y + 1;
    }

    // Find bottom bound
    let mut bottom = h;
    'bottom: for y in (0..h).rev() {
        for x in 0..w {
            if rgba.get_pixel(x, y)[3] > 0 {
                break 'bottom;
            }
        }
        bottom = y;
    }

    // Find left bound
    let mut left = 0u32;
    'left: for x in 0..w {
        for y in top..bottom {
            if rgba.get_pixel(x, y)[3] > 0 {
                break 'left;
            }
        }
        left = x + 1;
    }

    // Find right bound
    let mut right = w;
    'right: for x in (0..w).rev() {
        for y in top..bottom {
            if rgba.get_pixel(x, y)[3] > 0 {
                break 'right;
            }
        }
        right = x;
    }

    if left >= right || top >= bottom {
        return img.clone();
    }

    DynamicImage::ImageRgba8(
        image::imageops::crop_imm(&rgba, left, top, right - left, bottom - top).to_image(),
    )
}

/// Fit mode for resizing images.
#[derive(Debug, Clone, Copy)]
pub enum FitMode {
    /// Scale to fit within target, preserve aspect, center on canvas.
    Contain,
    /// Scale to fill target, crop overflow (centered).
    Crop,
    /// Resize ignoring aspect ratio.
    Stretch,
}

impl FitMode {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "crop" => FitMode::Crop,
            "stretch" => FitMode::Stretch,
            _ => FitMode::Contain,
        }
    }
}

/// Resize an image to fit within target dimensions using the specified mode.
pub fn fit_image(
    img: &DynamicImage,
    target_w: u32,
    target_h: u32,
    mode: FitMode,
    filter: FilterType,
) -> DynamicImage {
    match mode {
        FitMode::Contain => {
            let (iw, ih) = img.dimensions();
            let scale = f64::min(target_w as f64 / iw as f64, target_h as f64 / ih as f64);
            let new_w = (iw as f64 * scale).round() as u32;
            let new_h = (ih as f64 * scale).round() as u32;

            let resized = img.resize_exact(new_w.max(1), new_h.max(1), filter);

            // Center on target-sized canvas
            let mut canvas = RgbaImage::new(target_w, target_h);
            let x_offset = (target_w.saturating_sub(new_w)) / 2;
            let y_offset = (target_h.saturating_sub(new_h)) / 2;
            image::imageops::overlay(
                &mut canvas,
                &resized.to_rgba8(),
                x_offset as i64,
                y_offset as i64,
            );
            DynamicImage::ImageRgba8(canvas)
        }
        FitMode::Crop => {
            let (iw, ih) = img.dimensions();
            let scale = f64::max(target_w as f64 / iw as f64, target_h as f64 / ih as f64);
            let new_w = (iw as f64 * scale).round() as u32;
            let new_h = (ih as f64 * scale).round() as u32;

            let resized = img.resize_exact(new_w.max(1), new_h.max(1), filter);

            // Crop to target from center
            let x_offset = (new_w.saturating_sub(target_w)) / 2;
            let y_offset = (new_h.saturating_sub(target_h)) / 2;
            resized.crop_imm(x_offset, y_offset, target_w, target_h)
        }
        FitMode::Stretch => img.resize_exact(target_w.max(1), target_h.max(1), filter),
    }
}

/// Get the image filter type from a config string.
pub fn scaling_filter(method: &str) -> FilterType {
    match method.to_lowercase().as_str() {
        "sharp" | "nearest" => FilterType::Nearest,
        _ => FilterType::Lanczos3,
    }
}

/// Sample a frame color from an image using grid-based RGB averaging with HSL adjustment.
/// Matches ES-DE's MiximageGenerator::sampleFrameColor().
pub fn sample_frame_color(img: &RgbaImage, canvas_width: u32) -> Rgba<u8> {
    let spacing = (canvas_width as f64 * 0.03125).max(1.0) as u32;
    let (w, h) = img.dimensions();

    let mut r_sum: u64 = 0;
    let mut g_sum: u64 = 0;
    let mut b_sum: u64 = 0;
    let mut count: u64 = 0;

    let mut y = 0;
    while y < h {
        let mut x = 0;
        while x < w {
            let px = img.get_pixel(x, y);
            if px[3] > 0 {
                r_sum += px[0] as u64;
                g_sum += px[1] as u64;
                b_sum += px[2] as u64;
                count += 1;
            }
            x += spacing;
        }
        y += spacing;
    }

    if count == 0 {
        return Rgba([128, 128, 128, 255]);
    }

    let r_avg = (r_sum / count) as f64 / 255.0;
    let g_avg = (g_sum / count) as f64 / 255.0;
    let b_avg = (b_sum / count) as f64 / 255.0;

    // Convert to HSL
    let (h, s, l) = rgb_to_hsl(r_avg, g_avg, b_avg);

    // Adjust: saturation *= 0.9, lightness *= 1.25 (clamped min 0.10)
    let s_adj = s * 0.9;
    let l_adj = (l * 1.25).max(0.10);

    // Convert back to RGB
    let (r, g, b) = hsl_to_rgb(h, s_adj, l_adj);

    Rgba([(r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8, 255])
}

/// Add a drop shadow to an image.
/// Expands canvas by shadow_size on all sides, renders shadow from alpha, applies box blur.
pub fn add_drop_shadow(
    img: &RgbaImage,
    shadow_size: u32,
    opacity: f32,
    blur_iterations: u32,
) -> RgbaImage {
    let (w, h) = img.dimensions();
    let new_w = w + shadow_size * 2;
    let new_h = h + shadow_size * 2;

    // Create shadow layer from alpha channel
    let mut shadow = RgbaImage::new(new_w, new_h);
    let shadow_alpha = (opacity * 255.0) as u8;

    // Offset shadow down-right by half shadow_size
    let offset = (shadow_size / 2) as i64;

    for y in 0..h {
        for x in 0..w {
            let px = img.get_pixel(x, y);
            if px[3] > 0 {
                let sx = x as i64 + shadow_size as i64 + offset;
                let sy = y as i64 + shadow_size as i64 + offset;
                if sx >= 0 && sx < new_w as i64 && sy >= 0 && sy < new_h as i64 {
                    let alpha = ((px[3] as f32 / 255.0) * shadow_alpha as f32) as u8;
                    shadow.put_pixel(sx as u32, sy as u32, Rgba([0, 0, 0, alpha]));
                }
            }
        }
    }

    // Box blur the shadow
    for _ in 0..blur_iterations {
        shadow = box_blur_alpha(&shadow);
    }

    // Composite original on top
    image::imageops::overlay(&mut shadow, img, shadow_size as i64, shadow_size as i64);

    shadow
}

/// Simple box blur on the alpha channel only (3x3 kernel).
fn box_blur_alpha(img: &RgbaImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mut result = RgbaImage::new(w, h);

    for y in 0..h {
        for x in 0..w {
            let mut sum: u32 = 0;
            let mut count: u32 = 0;

            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                        sum += img.get_pixel(nx as u32, ny as u32)[3] as u32;
                        count += 1;
                    }
                }
            }

            let original = img.get_pixel(x, y);
            let blurred_alpha = (sum / count) as u8;
            // Keep shadow color (black) but blur alpha
            if original[3] > 0 || blurred_alpha > 0 {
                result.put_pixel(x, y, Rgba([0, 0, 0, blurred_alpha]));
            }
        }
    }

    result
}

// --- Color space conversion ---

fn rgb_to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = f64::midpoint(max, min);

    if (max - min).abs() < f64::EPSILON {
        return (0.0, 0.0, l);
    }

    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };

    let h = if (max - r).abs() < f64::EPSILON {
        let mut h = (g - b) / d;
        if g < b {
            h += 6.0;
        }
        h
    } else if (max - g).abs() < f64::EPSILON {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };

    (h / 6.0, s, l)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    if s.abs() < f64::EPSILON {
        return (l, l, l);
    }

    let q = if l < 0.5 { l * (1.0 + s) } else { l.mul_add(-s, l + s) };
    let p = 2.0f64.mul_add(l, -q);

    let r = hue_to_rgb(p, q, h + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h);
    let b = hue_to_rgb(p, q, h - 1.0 / 3.0);

    (r, g, b)
}

fn hue_to_rgb(p: f64, q: f64, mut t: f64) -> f64 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return ((q - p) * 6.0).mul_add(t, p);
    }
    if t < 1.0 / 2.0 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return ((q - p) * (2.0 / 3.0 - t)).mul_add(6.0, p);
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fit_mode_from_str() {
        assert!(matches!(FitMode::from_str("crop"), FitMode::Crop));
        assert!(matches!(FitMode::from_str("Crop"), FitMode::Crop));
        assert!(matches!(FitMode::from_str("stretch"), FitMode::Stretch));
        assert!(matches!(FitMode::from_str("contain"), FitMode::Contain));
        assert!(matches!(FitMode::from_str("anything"), FitMode::Contain));
    }

    #[test]
    fn test_scaling_filter() {
        assert!(matches!(scaling_filter("sharp"), FilterType::Nearest));
        assert!(matches!(scaling_filter("nearest"), FilterType::Nearest));
        assert!(matches!(scaling_filter("smooth"), FilterType::Lanczos3));
        assert!(matches!(scaling_filter("unknown"), FilterType::Lanczos3));
    }

    #[test]
    fn test_rgb_to_hsl_pure_red() {
        let (h, s, l) = rgb_to_hsl(1.0, 0.0, 0.0);
        assert!((h - 0.0).abs() < 0.01);
        assert!((s - 1.0).abs() < 0.01);
        assert!((l - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_rgb_to_hsl_gray() {
        let (h, s, l) = rgb_to_hsl(0.5, 0.5, 0.5);
        assert!((s - 0.0).abs() < 0.01);
        assert!((l - 0.5).abs() < 0.01);
        let _ = h; // hue undefined for gray
    }

    #[test]
    fn test_rgb_to_hsl_white() {
        let (_, s, l) = rgb_to_hsl(1.0, 1.0, 1.0);
        assert!((s - 0.0).abs() < 0.01);
        assert!((l - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_rgb_to_hsl_black() {
        let (_, s, l) = rgb_to_hsl(0.0, 0.0, 0.0);
        assert!((s - 0.0).abs() < 0.01);
        assert!((l - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_hsl_to_rgb_roundtrip() {
        let original = (0.7, 0.3, 0.2);
        let (h, s, l) = rgb_to_hsl(original.0, original.1, original.2);
        let (r, g, b) = hsl_to_rgb(h, s, l);
        assert!((r - original.0).abs() < 0.01, "r: {} vs {}", r, original.0);
        assert!((g - original.1).abs() < 0.01, "g: {} vs {}", g, original.1);
        assert!((b - original.2).abs() < 0.01, "b: {} vs {}", b, original.2);
    }

    #[test]
    fn test_remove_transparent_padding() {
        // 4x4 image with a 2x2 opaque center
        let mut img = RgbaImage::new(4, 4);
        // Everything starts transparent (all zeros)
        // Put opaque pixels at (1,1), (2,1), (1,2), (2,2)
        img.put_pixel(1, 1, Rgba([255, 0, 0, 255]));
        img.put_pixel(2, 1, Rgba([255, 0, 0, 255]));
        img.put_pixel(1, 2, Rgba([0, 255, 0, 255]));
        img.put_pixel(2, 2, Rgba([0, 255, 0, 255]));

        let result = remove_transparent_padding(&DynamicImage::ImageRgba8(img));
        let (w, h) = result.dimensions();
        assert_eq!(w, 2);
        assert_eq!(h, 2);
    }

    #[test]
    fn test_remove_transparent_padding_fully_opaque() {
        let img = RgbaImage::from_pixel(3, 3, Rgba([128, 128, 128, 255]));
        let result = remove_transparent_padding(&DynamicImage::ImageRgba8(img));
        let (w, h) = result.dimensions();
        assert_eq!(w, 3);
        assert_eq!(h, 3);
    }

    #[test]
    fn test_remove_transparent_padding_fully_transparent() {
        let img = RgbaImage::new(3, 3);
        let result = remove_transparent_padding(&DynamicImage::ImageRgba8(img));
        // Returns original when no opaque pixels
        let (w, h) = result.dimensions();
        assert_eq!(w, 3);
        assert_eq!(h, 3);
    }

    #[test]
    fn test_remove_transparent_padding_zero_sized_image() {
        let img = RgbaImage::new(0, 0);
        let result = remove_transparent_padding(&DynamicImage::ImageRgba8(img));
        let (w, h) = result.dimensions();
        assert_eq!(w, 0);
        assert_eq!(h, 0);
    }

    #[test]
    fn test_fit_image_contain_preserves_aspect() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(200, 100, Rgba([255, 0, 0, 255])));
        let result = fit_image(&img, 100, 100, FitMode::Contain, FilterType::Nearest);
        let (w, h) = result.dimensions();
        assert_eq!(w, 100);
        assert_eq!(h, 100);
        // The 200x100 image scaled to fit 100x100 should be 100x50 centered
        // Check center pixel is red
        let rgba = result.to_rgba8();
        let center = rgba.get_pixel(50, 50);
        assert_eq!(center[0], 255); // Red
    }

    #[test]
    fn test_fit_image_stretch() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(50, 50, Rgba([0, 255, 0, 255])));
        let result = fit_image(&img, 100, 200, FitMode::Stretch, FilterType::Nearest);
        let (w, h) = result.dimensions();
        assert_eq!(w, 100);
        assert_eq!(h, 200);
    }

    #[test]
    fn test_fit_image_crop() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(200, 100, Rgba([255, 0, 0, 255])));
        let result = fit_image(&img, 50, 50, FitMode::Crop, FilterType::Nearest);
        let (w, h) = result.dimensions();
        assert_eq!(w, 50);
        assert_eq!(h, 50);
    }

    #[test]
    fn test_sample_frame_color_uniform() {
        let img = RgbaImage::from_pixel(100, 100, Rgba([200, 100, 50, 255]));
        let color = sample_frame_color(&img, 100);
        // Should return an adjusted version of the input color
        assert_eq!(color[3], 255); // Alpha always 255
                                   // Lightness is increased so the result should be brighter
                                   // The exact values depend on HSL math but it shouldn't be the same
    }

    #[test]
    fn test_sample_frame_color_transparent_returns_gray() {
        let img = RgbaImage::new(100, 100); // All transparent
        let color = sample_frame_color(&img, 100);
        assert_eq!(color, Rgba([128, 128, 128, 255]));
    }

    #[test]
    fn test_sample_frame_color_zero_canvas_width_uses_min_spacing() {
        let img = RgbaImage::from_pixel(2, 2, Rgba([64, 96, 128, 255]));
        let color = sample_frame_color(&img, 0);
        assert_eq!(color[3], 255);
    }

    #[test]
    fn test_add_drop_shadow_expands_canvas() {
        let img = RgbaImage::from_pixel(10, 10, Rgba([255, 0, 0, 255]));
        let shadow_size = 4;
        let result = add_drop_shadow(&img, shadow_size, 0.6, 2);
        let (w, h) = result.dimensions();
        assert_eq!(w, 10 + shadow_size * 2);
        assert_eq!(h, 10 + shadow_size * 2);
    }

    #[test]
    fn test_add_drop_shadow_preserves_original() {
        let img = RgbaImage::from_pixel(10, 10, Rgba([255, 0, 0, 255]));
        let shadow_size = 4;
        let result = add_drop_shadow(&img, shadow_size, 0.6, 2);
        // Original image should be at (shadow_size, shadow_size)
        let px = result.get_pixel(shadow_size, shadow_size);
        assert_eq!(px[0], 255);
        assert_eq!(px[1], 0);
        assert_eq!(px[2], 0);
        assert_eq!(px[3], 255);
    }

    #[test]
    fn test_add_drop_shadow_zero_blur_iterations() {
        let img = RgbaImage::from_pixel(4, 4, Rgba([10, 20, 30, 255]));
        let result = add_drop_shadow(&img, 2, 0.5, 0);
        let (w, h) = result.dimensions();
        assert_eq!(w, 8);
        assert_eq!(h, 8);
    }

    #[test]
    fn test_hue_to_rgb_wraps_out_of_range_t() {
        let p = 0.2;
        let q = 0.8;
        let wrapped_low = hue_to_rgb(p, q, -0.1);
        let wrapped_high = hue_to_rgb(p, q, 1.1);
        assert!((wrapped_low - hue_to_rgb(p, q, 0.9)).abs() < 1e-9);
        assert!((wrapped_high - hue_to_rgb(p, q, 0.1)).abs() < 1e-9);
    }

    #[test]
    fn test_hue_to_rgb_piecewise_segments() {
        let p = 0.2;
        let q = 0.8;
        let first = hue_to_rgb(p, q, 0.10);
        assert!((first - ((q - p) * 6.0).mul_add(0.10, p)).abs() < 1e-9);
        assert!((hue_to_rgb(p, q, 0.40) - q).abs() < 1e-9);
        let third = hue_to_rgb(p, q, 0.60);
        assert!(third > p && third < q);
        assert!((hue_to_rgb(p, q, 0.90) - p).abs() < 1e-9);
    }
}
