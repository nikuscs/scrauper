use std::path::Path;

use anyhow::{Context, Result};
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};

use crate::config::{Miximage as MiximageConfig, MiximageLayout, MiximageScreenshot};
use crate::media::pillarbox;
use crate::media::processing::{
    add_drop_shadow, fit_image, remove_transparent_padding, sample_frame_color, scaling_filter,
    FitMode,
};

// --- Layout constants (base values at 1x multiplier = 640x480) ---

const SCREENSHOT_W: u32 = 530;
const SCREENSHOT_H: u32 = 400;
const SCREENSHOT_X_OFFSET: u32 = 20; // Offset right of center
const FRAME_WIDTH: u32 = 6;
const CORNER_RADIUS: u32 = 8;
const CORNER_OFFSET: u32 = 2;

const MARQUEE_W: u32 = 310;
const MARQUEE_H: u32 = 230;

const SHADOW_SIZE: u32 = 6;
const SHADOW_OPACITY: f32 = 0.6;
const SHADOW_BLUR_ITERATIONS: u32 = 4;

const PHYS_MEDIA_MARGIN: u32 = 16;

// Aspect ratio thresholds for switching fit modes
const HIGH_HORIZONTAL_THRESHOLD: f64 = 1.6;
const HIGH_VERTICAL_THRESHOLD: f64 = 1.05;
const LOW_HORIZONTAL_THRESHOLD: f64 = 1.375;
const LOW_VERTICAL_THRESHOLD: f64 = 1.275;

/// Box sizes: (total_w, total_h, cover_w)
fn box_dimensions(size: &str) -> (u32, u32, u32) {
    match size {
        "small" => (264, 254, 212),
        "large" => (372, 360, 300),
        _ => (310, 300, 250), // medium (default)
    }
}

/// Physical media sizes: (w, h)
fn phys_media_dimensions(size: &str) -> (u32, u32) {
    match size {
        "small" => (120, 96),
        "large" => (196, 156),
        _ => (150, 120), // medium (default)
    }
}

/// Parse resolution to get the multiplier (1x, 2x, 3x).
fn resolution_multiplier(width: u32, height: u32) -> u32 {
    if width >= 1920 && height >= 1440 {
        3
    } else if width >= 1280 && height >= 960 {
        2
    } else {
        1
    }
}

/// Miximage generator matching ES-DE's MiximageGenerator.cpp compositing.
pub struct MiximageGenerator {
    canvas_w: u32,
    canvas_h: u32,
    mult: u32,
    screenshot_config: MiximageScreenshot,
    layout: MiximageLayout,
    remove_letterboxes: bool,
    remove_pillarboxes: bool,
    #[allow(dead_code)]
    format: String,
}

impl MiximageGenerator {
    pub fn new(config: &MiximageConfig, remove_pillarboxes: bool) -> Self {
        let mult = resolution_multiplier(config.width, config.height);
        Self {
            canvas_w: config.width,
            canvas_h: config.height,
            mult,
            screenshot_config: config.screenshot.clone(),
            layout: config.layout.clone(),
            remove_letterboxes: remove_pillarboxes, // Reuse config flag
            remove_pillarboxes,
            format: config.format.clone(),
        }
    }

    /// Generate a miximage composite from available media files.
    pub fn generate(
        &self,
        screenshot_path: &Path,
        marquee_path: Option<&Path>,
        box_path: Option<&Path>,
        physical_media_path: Option<&Path>,
        output_path: &Path,
    ) -> Result<()> {
        let m = self.mult;

        // Create blank canvas
        let mut canvas = RgbaImage::new(self.canvas_w, self.canvas_h);

        // --- Load and process screenshot ---
        let screenshot = image::open(screenshot_path)
            .with_context(|| format!("Failed to load screenshot: {}", screenshot_path.display()))?;

        let screenshot = self.process_screenshot(screenshot);
        let screenshot_rgba = screenshot.to_rgba8();

        // Calculate screenshot position (centered + offset right)
        let ss_w = SCREENSHOT_W * m;
        let ss_h = SCREENSHOT_H * m;
        let ss_x = (self.canvas_w - ss_w) / 2 + SCREENSHOT_X_OFFSET * m;
        let ss_y = (self.canvas_h - ss_h) / 2;

        // --- Sample frame color ---
        let frame_color = sample_frame_color(&screenshot_rgba, self.canvas_w);

        // --- Draw frame (rounded rectangle) ---
        let fw = FRAME_WIDTH * m;
        let cr = CORNER_RADIUS * m;
        let co = CORNER_OFFSET * m;
        self.draw_frame(&mut canvas, ss_x, ss_y, ss_w, ss_h, fw, cr, co, frame_color);

        // --- Draw screenshot on top of frame ---
        let fitted_ss = fit_image(
            &screenshot,
            ss_w,
            ss_h,
            self.screenshot_fit_mode(&screenshot),
            scaling_filter(&self.screenshot_config.scaling),
        );
        // Force fully opaque
        let mut ss_rgba = fitted_ss.to_rgba8();
        for px in ss_rgba.pixels_mut() {
            px[3] = 255;
        }
        image::imageops::overlay(&mut canvas, &ss_rgba, ss_x as i64, ss_y as i64);

        // --- Process and draw marquee (top-right) ---
        if let Some(path) = marquee_path {
            if let Ok(marquee) = image::open(path) {
                self.draw_marquee(&mut canvas, marquee, m);
            }
        }

        // --- Process and draw box (bottom-left) ---
        if let Some(path) = box_path {
            if let Ok(box_img) = image::open(path) {
                let box_x = self.draw_box(&mut canvas, box_img, m);

                // --- Process and draw physical media (right of box) ---
                if self.layout.include_physical_media {
                    if let Some(pm_path) = physical_media_path {
                        if let Ok(pm_img) = image::open(pm_path) {
                            self.draw_physical_media(&mut canvas, pm_img, m, box_x);
                        }
                    }
                }
            }
        }

        // --- Save ---
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        canvas.save(output_path)?;

        Ok(())
    }

    /// Process screenshot: remove bars, determine fit mode.
    fn process_screenshot(&self, mut img: DynamicImage) -> DynamicImage {
        if self.remove_letterboxes {
            img = pillarbox::crop_letterboxes(&img);
        }
        if self.remove_pillarboxes {
            img = pillarbox::crop_pillarboxes(&img);
        }
        img
    }

    /// Determine the fit mode for a screenshot based on its aspect ratio.
    fn screenshot_fit_mode(&self, img: &DynamicImage) -> FitMode {
        let (w, h) = img.dimensions();
        if w == 0 || h == 0 {
            return FitMode::Contain;
        }

        let aspect = w as f64 / h as f64;

        if aspect >= 1.0 {
            // Horizontal screenshot
            let threshold = if self.screenshot_config.aspect_ratio_threshold > 1.5 {
                HIGH_HORIZONTAL_THRESHOLD
            } else {
                LOW_HORIZONTAL_THRESHOLD
            };
            if aspect > threshold {
                FitMode::from_str(&self.screenshot_config.horizontal_fit)
            } else {
                FitMode::Contain // Within threshold, just resize
            }
        } else {
            // Vertical screenshot
            let inv_aspect = h as f64 / w as f64;
            let threshold = if self.screenshot_config.aspect_ratio_threshold > 1.5 {
                HIGH_VERTICAL_THRESHOLD
            } else {
                LOW_VERTICAL_THRESHOLD
            };
            if inv_aspect > threshold {
                FitMode::from_str(&self.screenshot_config.vertical_fit)
            } else {
                FitMode::Contain
            }
        }
    }

    /// Draw a rounded rectangle frame around the screenshot area.
    #[allow(clippy::unused_self)]
    fn draw_frame(
        &self,
        canvas: &mut RgbaImage,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        frame_w: u32,
        radius: u32,
        offset: u32,
        color: Rgba<u8>,
    ) {
        // Top and bottom bars (inset by corner offset)
        let top_x_start = x + offset;
        let top_x_end = x + w - offset;
        fill_rect(
            canvas,
            top_x_start,
            y.saturating_sub(frame_w),
            top_x_end - top_x_start,
            frame_w,
            color,
        );
        fill_rect(canvas, top_x_start, y + h, top_x_end - top_x_start, frame_w, color);

        // Left and right bars (inset by corner offset)
        let left_y_start = y + offset;
        let left_y_end = y + h - offset;
        fill_rect(
            canvas,
            x.saturating_sub(frame_w),
            left_y_start,
            frame_w,
            left_y_end - left_y_start,
            color,
        );
        fill_rect(canvas, x + w, left_y_start, frame_w, left_y_end - left_y_start, color);

        // Four corner circles
        draw_filled_circle(canvas, x + offset, y + offset, radius, color); // top-left
        draw_filled_circle(canvas, x + w - offset - 1, y + offset, radius, color); // top-right
        draw_filled_circle(canvas, x + offset, y + h - offset - 1, radius, color); // bottom-left
        draw_filled_circle(canvas, x + w - offset - 1, y + h - offset - 1, radius, color);
        // bottom-right
    }

    /// Draw marquee in the top-right corner.
    fn draw_marquee(&self, canvas: &mut RgbaImage, marquee: DynamicImage, m: u32) {
        let marquee = remove_transparent_padding(&marquee);
        let (mw, mh) = marquee.dimensions();
        if mw == 0 || mh == 0 {
            return;
        }

        let target_w = MARQUEE_W * m;
        let target_h = MARQUEE_H * m;

        // Dynamic sizing: wider logos get more space
        let width_ratio = mw as f64 / mh as f64;
        let width_modifier = (0.5 + width_ratio / 6.5).clamp(0.0, 1.0);
        let adj_target_w = (target_w as f64 * width_modifier).round() as u32;

        // Fit within adjusted target, always use Lanczos3
        let fitted = fit_image(
            &marquee,
            adj_target_w.max(1),
            target_h,
            FitMode::Contain,
            FilterType::Lanczos3,
        );

        let shadowed = add_drop_shadow(
            &fitted.to_rgba8(),
            SHADOW_SIZE * m,
            SHADOW_OPACITY,
            SHADOW_BLUR_ITERATIONS,
        );

        // Position: top-right
        let (sw, _sh) = shadowed.dimensions();
        let x = self.canvas_w.saturating_sub(sw);
        image::imageops::overlay(canvas, &shadowed, x as i64, 0);
    }

    /// Draw box art in the bottom-left corner. Returns the x-end position for physical media.
    fn draw_box(&self, canvas: &mut RgbaImage, box_img: DynamicImage, m: u32) -> u32 {
        let box_img = remove_transparent_padding(&box_img);
        let (bw, bh) = box_img.dimensions();
        if bw == 0 || bh == 0 {
            return 0;
        }

        let (target_w, target_h, _cover_w) = box_dimensions(&self.layout.box_size);
        let target_w = target_w * m;
        let target_h = target_h * m;

        // Scale to fit within target dimensions
        let fitted =
            fit_image(&box_img, target_w, target_h, FitMode::Contain, FilterType::Lanczos3);

        let shadowed = add_drop_shadow(
            &fitted.to_rgba8(),
            SHADOW_SIZE * m,
            SHADOW_OPACITY,
            SHADOW_BLUR_ITERATIONS,
        );

        let (sw, sh) = shadowed.dimensions();

        // Position: bottom-left
        let y = self.canvas_h.saturating_sub(sh);
        image::imageops::overlay(canvas, &shadowed, 0, y as i64);

        sw // Return width for physical media positioning
    }

    /// Draw physical media to the right of the box art.
    fn draw_physical_media(
        &self,
        canvas: &mut RgbaImage,
        pm_img: DynamicImage,
        m: u32,
        box_end_x: u32,
    ) {
        let pm_img = remove_transparent_padding(&pm_img);
        let (pw, ph) = pm_img.dimensions();
        if pw == 0 || ph == 0 {
            return;
        }

        let (target_w, target_h) = phys_media_dimensions(&self.layout.physical_media_size);
        let target_w = target_w * m;
        let target_h = target_h * m;

        // Scale to fit
        let scale = f64::min(target_w as f64 / pw as f64, target_h as f64 / ph as f64);
        let new_w = (pw as f64 * scale).round() as u32;
        let new_h = (ph as f64 * scale).round() as u32;

        let resized = pm_img.resize_exact(new_w.max(1), new_h.max(1), FilterType::Lanczos3);

        let shadowed = add_drop_shadow(
            &resized.to_rgba8(),
            SHADOW_SIZE * m,
            SHADOW_OPACITY,
            SHADOW_BLUR_ITERATIONS,
        );

        let (_sw, sh) = shadowed.dimensions();

        // Position: right of box, bottom-aligned
        let x = box_end_x + PHYS_MEDIA_MARGIN * m;
        let y = self.canvas_h.saturating_sub(sh);
        image::imageops::overlay(canvas, &shadowed, x as i64, y as i64);
    }
}

// --- Drawing primitives ---

fn fill_rect(canvas: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, color: Rgba<u8>) {
    let (cw, ch) = canvas.dimensions();
    for dy in 0..h {
        for dx in 0..w {
            let px = x + dx;
            let py = y + dy;
            if px < cw && py < ch {
                canvas.put_pixel(px, py, color);
            }
        }
    }
}

fn draw_filled_circle(canvas: &mut RgbaImage, cx: u32, cy: u32, radius: u32, color: Rgba<u8>) {
    let (cw, ch) = canvas.dimensions();
    let r = radius as i32;

    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                let px = cx as i32 + dx;
                let py = cy as i32 + dy;
                if px >= 0 && px < cw as i32 && py >= 0 && py < ch as i32 {
                    canvas.put_pixel(px as u32, py as u32, color);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_box_dimensions_small() {
        assert_eq!(box_dimensions("small"), (264, 254, 212));
    }

    #[test]
    fn test_box_dimensions_medium() {
        assert_eq!(box_dimensions("medium"), (310, 300, 250));
    }

    #[test]
    fn test_box_dimensions_large() {
        assert_eq!(box_dimensions("large"), (372, 360, 300));
    }

    #[test]
    fn test_box_dimensions_unknown_defaults_medium() {
        assert_eq!(box_dimensions("unknown"), box_dimensions("medium"));
    }

    #[test]
    fn test_phys_media_dimensions() {
        assert_eq!(phys_media_dimensions("small"), (120, 96));
        assert_eq!(phys_media_dimensions("medium"), (150, 120));
        assert_eq!(phys_media_dimensions("large"), (196, 156));
        assert_eq!(phys_media_dimensions("unknown"), (150, 120));
    }

    #[test]
    fn test_resolution_multiplier_1x() {
        assert_eq!(resolution_multiplier(640, 480), 1);
        assert_eq!(resolution_multiplier(800, 600), 1);
    }

    #[test]
    fn test_resolution_multiplier_2x() {
        assert_eq!(resolution_multiplier(1280, 960), 2);
        assert_eq!(resolution_multiplier(1600, 1200), 2);
    }

    #[test]
    fn test_resolution_multiplier_3x() {
        assert_eq!(resolution_multiplier(1920, 1440), 3);
        assert_eq!(resolution_multiplier(2560, 1920), 3);
    }

    #[test]
    fn test_fill_rect_within_bounds() {
        let mut canvas = RgbaImage::new(10, 10);
        let color = Rgba([255, 0, 0, 255]);
        fill_rect(&mut canvas, 2, 2, 3, 3, color);
        assert_eq!(*canvas.get_pixel(3, 3), color);
        assert_eq!(*canvas.get_pixel(0, 0), Rgba([0, 0, 0, 0]));
    }

    #[test]
    fn test_fill_rect_clipped() {
        let mut canvas = RgbaImage::new(5, 5);
        let color = Rgba([255, 0, 0, 255]);
        fill_rect(&mut canvas, 3, 3, 10, 10, color);
        assert_eq!(*canvas.get_pixel(4, 4), color);
    }

    #[test]
    fn test_draw_filled_circle() {
        let mut canvas = RgbaImage::new(20, 20);
        let color = Rgba([0, 255, 0, 255]);
        draw_filled_circle(&mut canvas, 10, 10, 3, color);
        assert_eq!(*canvas.get_pixel(10, 10), color);
        assert_eq!(*canvas.get_pixel(0, 0), Rgba([0, 0, 0, 0]));
    }

    #[test]
    fn test_miximage_generator_new() {
        let config = crate::config::Miximage::default();
        let gen = MiximageGenerator::new(&config, false);
        assert_eq!(gen.canvas_w, 1280);
        assert_eq!(gen.canvas_h, 960);
        assert_eq!(gen.mult, 2);
    }

    #[test]
    fn test_screenshot_fit_mode_horizontal_wide() {
        let config = crate::config::Miximage::default();
        let gen = MiximageGenerator::new(&config, false);
        let img = DynamicImage::new_rgba8(1920, 1080);
        let mode = gen.screenshot_fit_mode(&img);
        assert!(matches!(mode, FitMode::Crop));
    }

    #[test]
    fn test_screenshot_fit_mode_near_square() {
        let config = crate::config::Miximage::default();
        let gen = MiximageGenerator::new(&config, false);
        let img = DynamicImage::new_rgba8(320, 240);
        let mode = gen.screenshot_fit_mode(&img);
        assert!(matches!(mode, FitMode::Contain));
    }

    #[test]
    fn test_screenshot_fit_mode_vertical_tall() {
        let config = crate::config::Miximage::default();
        let gen = MiximageGenerator::new(&config, false);
        let img = DynamicImage::new_rgba8(240, 400);
        let mode = gen.screenshot_fit_mode(&img);
        assert!(matches!(mode, FitMode::Contain));
    }

    #[test]
    fn test_screenshot_fit_mode_zero_dims() {
        let config = crate::config::Miximage::default();
        let gen = MiximageGenerator::new(&config, false);
        let img = DynamicImage::new_rgba8(0, 0);
        assert!(matches!(gen.screenshot_fit_mode(&img), FitMode::Contain));
    }

    #[test]
    fn test_generate_creates_output() {
        let tmp = tempfile::TempDir::new().unwrap();
        let ss = DynamicImage::new_rgba8(320, 240);
        let ss_path = tmp.path().join("screenshot.png");
        ss.save(&ss_path).unwrap();

        let output_path = tmp.path().join("output").join("miximage.png");
        let config = crate::config::Miximage::default();
        let gen = MiximageGenerator::new(&config, false);
        gen.generate(&ss_path, None, None, None, &output_path).unwrap();

        assert!(output_path.exists());
        let result = image::open(&output_path).unwrap();
        assert_eq!(result.width(), 1280);
        assert_eq!(result.height(), 960);
    }

    #[test]
    fn test_generate_with_all_companion_media() {
        let tmp = tempfile::TempDir::new().unwrap();

        // Create screenshot
        let ss = DynamicImage::new_rgba8(320, 240);
        let ss_path = tmp.path().join("screenshot.png");
        ss.save(&ss_path).unwrap();

        // Create marquee
        let marquee = DynamicImage::new_rgba8(200, 50);
        let marquee_path = tmp.path().join("marquee.png");
        marquee.save(&marquee_path).unwrap();

        // Create box art
        let box_art = DynamicImage::new_rgba8(150, 200);
        let box_path = tmp.path().join("box.png");
        box_art.save(&box_path).unwrap();

        // Create physical media
        let pm = DynamicImage::new_rgba8(100, 80);
        let pm_path = tmp.path().join("pm.png");
        pm.save(&pm_path).unwrap();

        let output_path = tmp.path().join("output").join("miximage.png");
        let config = crate::config::Miximage::default();
        let gen = MiximageGenerator::new(&config, false);
        gen.generate(&ss_path, Some(&marquee_path), Some(&box_path), Some(&pm_path), &output_path)
            .unwrap();

        assert!(output_path.exists());
        let result = image::open(&output_path).unwrap();
        assert_eq!(result.width(), 1280);
        assert_eq!(result.height(), 960);
    }

    #[test]
    fn test_generate_with_marquee_only() {
        let tmp = tempfile::TempDir::new().unwrap();

        let ss = DynamicImage::new_rgba8(320, 240);
        let ss_path = tmp.path().join("screenshot.png");
        ss.save(&ss_path).unwrap();

        let marquee = DynamicImage::new_rgba8(400, 100);
        let marquee_path = tmp.path().join("marquee.png");
        marquee.save(&marquee_path).unwrap();

        let output_path = tmp.path().join("miximage.png");
        let config = crate::config::Miximage::default();
        let gen = MiximageGenerator::new(&config, false);
        gen.generate(&ss_path, Some(&marquee_path), None, None, &output_path).unwrap();
        assert!(output_path.exists());
    }

    #[test]
    fn test_generate_with_box_only() {
        let tmp = tempfile::TempDir::new().unwrap();

        let ss = DynamicImage::new_rgba8(320, 240);
        let ss_path = tmp.path().join("screenshot.png");
        ss.save(&ss_path).unwrap();

        let box_art = DynamicImage::new_rgba8(200, 300);
        let box_path = tmp.path().join("box.png");
        box_art.save(&box_path).unwrap();

        let output_path = tmp.path().join("miximage.png");
        let config = crate::config::Miximage::default();
        let gen = MiximageGenerator::new(&config, false);
        gen.generate(&ss_path, None, Some(&box_path), None, &output_path).unwrap();
        assert!(output_path.exists());
    }

    #[test]
    fn test_process_screenshot_with_pillarbox_removal() {
        let config = crate::config::Miximage::default();
        let gen = MiximageGenerator::new(&config, true);
        assert!(gen.remove_pillarboxes);
        assert!(gen.remove_letterboxes);
    }

    #[test]
    fn test_screenshot_fit_mode_very_wide() {
        let mut config = crate::config::Miximage::default();
        config.screenshot.aspect_ratio_threshold = 1.6; // High threshold
        let gen = MiximageGenerator::new(&config, false);
        // Very wide image (3:1 aspect ratio > HIGH_HORIZONTAL_THRESHOLD of 1.6)
        let img = DynamicImage::new_rgba8(900, 300);
        let mode = gen.screenshot_fit_mode(&img);
        // With high threshold and aspect > 1.6, uses horizontal_fit
        assert!(matches!(mode, FitMode::Contain | FitMode::Crop));
    }

    #[test]
    fn test_screenshot_fit_mode_vertical_with_high_threshold() {
        let mut config = crate::config::Miximage::default();
        config.screenshot.aspect_ratio_threshold = 1.6;
        let gen = MiximageGenerator::new(&config, false);
        // Tall image with high inv_aspect
        let img = DynamicImage::new_rgba8(200, 600);
        let mode = gen.screenshot_fit_mode(&img);
        assert!(matches!(mode, FitMode::Contain | FitMode::Crop));
    }

    #[test]
    fn test_screenshot_fit_mode_low_threshold_wide() {
        let mut config = crate::config::Miximage::default();
        config.screenshot.aspect_ratio_threshold = 1.2; // Low threshold
        let gen = MiximageGenerator::new(&config, false);
        // Wide image with aspect > LOW_HORIZONTAL_THRESHOLD (1.375)
        let img = DynamicImage::new_rgba8(600, 400);
        let mode = gen.screenshot_fit_mode(&img);
        // aspect = 1.5 > 1.375 with low threshold path
        assert!(matches!(mode, FitMode::Contain | FitMode::Crop));
    }

    #[test]
    fn test_resolution_multiplier_edge_cases() {
        assert_eq!(resolution_multiplier(1279, 960), 1);
        assert_eq!(resolution_multiplier(1280, 959), 1);
        assert_eq!(resolution_multiplier(1919, 1440), 2);
        assert_eq!(resolution_multiplier(1920, 1439), 2);
    }

    #[test]
    fn test_box_dimensions_all_sizes() {
        let (sw, sh, sc) = box_dimensions("small");
        let (mw, mh, mc) = box_dimensions("medium");
        let (lw, lh, lc) = box_dimensions("large");
        assert!(sw < mw && mw < lw);
        assert!(sh < mh && mh < lh);
        assert!(sc < mc && mc < lc);
    }

    #[test]
    fn test_generator_1x_resolution() {
        let config = crate::config::Miximage {
            width: 640,
            height: 480,
            ..crate::config::Miximage::default()
        };
        let gen = MiximageGenerator::new(&config, false);
        assert_eq!(gen.mult, 1);
    }

    #[test]
    fn test_generator_3x_resolution() {
        let config = crate::config::Miximage {
            width: 1920,
            height: 1440,
            ..crate::config::Miximage::default()
        };
        let gen = MiximageGenerator::new(&config, false);
        assert_eq!(gen.mult, 3);
    }
}
