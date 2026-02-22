use image::{DynamicImage, Pixel};

/// Tolerance for comparing pixel colors when detecting bars (0-255 scale).
const COLOR_TOLERANCE: u8 = 10;

/// Minimum fraction of the image that bars must cover to be detected.
const MIN_BAR_FRACTION: f64 = 0.02;

/// Detect and crop solid-color bars from the top and bottom of an image (letterboxing).
pub fn crop_letterboxes(img: &DynamicImage) -> DynamicImage {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();

    if w == 0 || h == 0 {
        return img.clone();
    }

    let min_bar_size = (h as f64 * MIN_BAR_FRACTION).ceil() as u32;

    // Detect top bar: sample rows from top, check if uniform color
    let top_bar = detect_uniform_rows(&rgba, 0, h, w, true);
    let bottom_bar = detect_uniform_rows(&rgba, 0, h, w, false);

    let top = if top_bar >= min_bar_size { top_bar } else { 0 };
    let bottom = if bottom_bar >= min_bar_size { h - bottom_bar } else { h };

    if top >= bottom || (top == 0 && bottom == h) {
        return img.clone();
    }

    img.crop_imm(0, top, w, bottom - top)
}

/// Detect and crop solid-color bars from the left and right of an image (pillarboxing).
pub fn crop_pillarboxes(img: &DynamicImage) -> DynamicImage {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();

    if w == 0 || h == 0 {
        return img.clone();
    }

    let min_bar_size = (w as f64 * MIN_BAR_FRACTION).ceil() as u32;

    let left_bar = detect_uniform_cols(&rgba, 0, w, h, true);
    let right_bar = detect_uniform_cols(&rgba, 0, w, h, false);

    let left = if left_bar >= min_bar_size { left_bar } else { 0 };
    let right = if right_bar >= min_bar_size { w - right_bar } else { w };

    if left >= right || (left == 0 && right == w) {
        return img.clone();
    }

    img.crop_imm(left, 0, right - left, h)
}

/// Count uniform-colored rows from top (forward=true) or bottom (forward=false).
fn detect_uniform_rows(
    img: &image::RgbaImage,
    _y_start: u32,
    y_end: u32,
    width: u32,
    from_top: bool,
) -> u32 {
    let mut count = 0;

    let range: Vec<u32> = if from_top { (0..y_end).collect() } else { (0..y_end).rev().collect() };

    for y in range {
        if is_uniform_row(img, y, width) {
            count += 1;
        } else {
            break;
        }
    }

    count
}

/// Count uniform-colored columns from left (forward=true) or right (forward=false).
fn detect_uniform_cols(
    img: &image::RgbaImage,
    _x_start: u32,
    x_end: u32,
    height: u32,
    from_left: bool,
) -> u32 {
    let mut count = 0;

    let range: Vec<u32> = if from_left { (0..x_end).collect() } else { (0..x_end).rev().collect() };

    for x in range {
        if is_uniform_col(img, x, height) {
            count += 1;
        } else {
            break;
        }
    }

    count
}

/// Check if a row has all pixels within tolerance of each other.
fn is_uniform_row(img: &image::RgbaImage, y: u32, width: u32) -> bool {
    if width == 0 {
        return true;
    }

    let reference = img.get_pixel(0, y);
    for x in 1..width {
        let px = img.get_pixel(x, y);
        if !pixels_within_tolerance(reference, px) {
            return false;
        }
    }
    true
}

/// Check if a column has all pixels within tolerance of each other.
fn is_uniform_col(img: &image::RgbaImage, x: u32, height: u32) -> bool {
    if height == 0 {
        return true;
    }

    let reference = img.get_pixel(x, 0);
    for y in 1..height {
        let px = img.get_pixel(x, y);
        if !pixels_within_tolerance(reference, px) {
            return false;
        }
    }
    true
}

/// Check if two pixels are within the color tolerance threshold.
fn pixels_within_tolerance(a: &image::Rgba<u8>, b: &image::Rgba<u8>) -> bool {
    let channels_a = a.channels();
    let channels_b = b.channels();

    for i in 0..3 {
        // Compare RGB only, ignore alpha
        let diff = (channels_a[i] as i16 - channels_b[i] as i16).unsigned_abs() as u8;
        if diff > COLOR_TOLERANCE {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, Rgba, RgbaImage};

    /// Create an image with uniform-color bars and varied content.
    /// Content pixels vary by position so rows/columns are NOT uniform.
    fn make_image_with_bars(
        w: u32,
        h: u32,
        bar_color: Rgba<u8>,
        top: u32,
        bottom: u32,
        left: u32,
        right: u32,
    ) -> DynamicImage {
        let mut img = RgbaImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let is_bar = y < top || y >= h - bottom || x < left || x >= w - right;
                let color = if is_bar {
                    bar_color
                } else {
                    // Varied content: each pixel differs so rows/cols are non-uniform
                    Rgba([(x % 200 + 50) as u8, (y % 200 + 50) as u8, 128, 255])
                };
                img.put_pixel(x, y, color);
            }
        }
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn test_pixels_within_tolerance_identical() {
        let a = Rgba([100, 150, 200, 255]);
        let b = Rgba([100, 150, 200, 255]);
        assert!(pixels_within_tolerance(&a, &b));
    }

    #[test]
    fn test_pixels_within_tolerance_close() {
        let a = Rgba([100, 150, 200, 255]);
        let b = Rgba([105, 145, 195, 255]);
        assert!(pixels_within_tolerance(&a, &b));
    }

    #[test]
    fn test_pixels_within_tolerance_too_far() {
        let a = Rgba([100, 150, 200, 255]);
        let b = Rgba([100, 150, 180, 255]); // diff of 20 on blue
        assert!(!pixels_within_tolerance(&a, &b));
    }

    #[test]
    fn test_pixels_within_tolerance_ignores_alpha() {
        let a = Rgba([100, 150, 200, 255]);
        let b = Rgba([100, 150, 200, 0]);
        assert!(pixels_within_tolerance(&a, &b));
    }

    #[test]
    fn test_crop_letterboxes_with_bars() {
        // 100x100 image with 20px black bars top and bottom, varied content
        let black = Rgba([0, 0, 0, 255]);
        let img = make_image_with_bars(100, 100, black, 20, 20, 0, 0);

        let result = crop_letterboxes(&img);
        let (w, h) = result.dimensions();
        assert_eq!(w, 100);
        assert_eq!(h, 60); // 100 - 20 - 20
    }

    #[test]
    fn test_crop_letterboxes_no_bars() {
        let mut img = RgbaImage::new(100, 100);
        for y in 0..100 {
            for x in 0..100 {
                img.put_pixel(x, y, Rgba([x as u8, y as u8, 128, 255]));
            }
        }
        let dyn_img = DynamicImage::ImageRgba8(img);
        let result = crop_letterboxes(&dyn_img);
        assert_eq!(result.dimensions(), (100, 100));
    }

    #[test]
    fn test_crop_pillarboxes_with_bars() {
        // 100x100 image with 15px black bars left and right, varied content
        let black = Rgba([0, 0, 0, 255]);
        let img = make_image_with_bars(100, 100, black, 0, 0, 15, 15);

        let result = crop_pillarboxes(&img);
        let (w, h) = result.dimensions();
        assert_eq!(w, 70); // 100 - 15 - 15
        assert_eq!(h, 100);
    }

    #[test]
    fn test_crop_pillarboxes_no_bars() {
        let mut img = RgbaImage::new(100, 100);
        for y in 0..100 {
            for x in 0..100 {
                img.put_pixel(x, y, Rgba([x as u8, y as u8, 128, 255]));
            }
        }
        let dyn_img = DynamicImage::ImageRgba8(img);
        let result = crop_pillarboxes(&dyn_img);
        assert_eq!(result.dimensions(), (100, 100));
    }

    #[test]
    fn test_crop_letterboxes_bars_too_small() {
        // 1px bars on a 100px image — below MIN_BAR_FRACTION (2%)
        let black = Rgba([0, 0, 0, 255]);
        let img = make_image_with_bars(100, 100, black, 1, 1, 0, 0);

        let result = crop_letterboxes(&img);
        assert_eq!(result.dimensions(), (100, 100)); // No crop
    }

    #[test]
    fn test_crop_letterboxes_empty_image() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(0, 0));
        let result = crop_letterboxes(&img);
        assert_eq!(result.dimensions(), (0, 0));
    }

    #[test]
    fn test_crop_pillarboxes_empty_image() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(0, 0));
        let result = crop_pillarboxes(&img);
        assert_eq!(result.dimensions(), (0, 0));
    }
}
