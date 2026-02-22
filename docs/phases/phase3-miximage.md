# Phase 3: Miximage Generation

## Goal
Pixel-accurate ES-DE miximage generation. Composites screenshot + marquee + box art + physical media into a single image matching ES-DE's MiximageGenerator.cpp output. Both inline (during scrape) and offline (`scrauper generate-miximages`).

## Dependencies
- Phase 2 complete (scrape pipeline downloads screenshots, box art, marquees, physical media)

## Files to Create/Modify

```
src/
  media/
    mod.rs
    miximage.rs
    pillarbox.rs
    processing.rs
```

## Steps

### 3.1 — Image processing utilities (`media/processing.rs`)
- [ ] `remove_transparent_padding(img) -> DynamicImage` — strip fully transparent rows/columns from edges
- [ ] `fit_image(img, target_w, target_h, mode, scaling) -> DynamicImage` — resize with contain/crop/stretch
  - `contain`: scale to fit, preserve aspect, center on target-sized canvas
  - `crop`: scale to fill, crop overflow (centered)
  - `stretch`: resize ignoring aspect ratio
- [ ] `scaling_filter(method) -> FilterType` — "sharp" → `Nearest`, "smooth" → `Lanczos3`
- [ ] `sample_frame_color(img, canvas_width) -> Rgba<u8>` — grid-based RGB sampling with HSL adjustment
  - Sample spacing: `canvas_width * 0.03125` pixels
  - Average RGB, convert to HSL
  - saturation *= 0.9, lightness *= 1.25 (clamped min 0.10)
  - Convert back to RGB, alpha = 255
- [ ] `add_drop_shadow(img, shadow_size, opacity, blur_iterations) -> RgbaImage`
  - Expand canvas by shadow_size on all sides
  - Render shadow from alpha channel, offset down-right
  - Apply box blur N iterations
  - Composite original on top

### 3.2 — Pillarbox/letterbox removal (`media/pillarbox.rs`)
- [ ] `crop_letterboxes(img) -> DynamicImage` — detect solid-color top/bottom bars, crop
- [ ] `crop_pillarboxes(img) -> DynamicImage` — detect solid-color left/right bars, crop
- [ ] Detection: sample edge columns/rows, check if all pixels within tolerance of each other
- [ ] Tolerance: allow small variation (noise) — compare against a threshold

### 3.3 — Miximage generator (`media/miximage.rs`)
- [ ] `MiximageGenerator` struct holding miximage config
- [ ] `generate(screenshot, marquee, box_art, physical_media, output_path) -> Result<()>`

**Resolution multiplier system:**
- [ ] Parse resolution string → (width, height, multiplier): "640x480"→1, "1280x960"→2, "1920x1440"→3

**Layout constants (all * multiplier):**
- [ ] Screenshot: 530w × 400h, offset 20px right of center, frame 6px, corner radius 8px, corner offset 2px
- [ ] Marquee target: 310w × 230h
- [ ] Box sizes: small=264×254 (cover 212w), medium=310×300 (cover 250w), large=372×360 (cover 300w)
- [ ] Physical media sizes: small=120×96, medium=150×120, large=196×156
- [ ] Shadow size: 6px on all overlays
- [ ] Physical media margin from box: 16px

**Compositing pipeline:**
- [ ] Create blank RGBA canvas (width × height, all zeros)
- [ ] **Load + process screenshot:**
  - If `remove_letterboxes`: crop letterboxes
  - If `remove_pillarboxes`: crop pillarboxes
  - Compute aspect ratio
  - If aspect >= 1.0 → use `horizontal_fit`, else → use `vertical_fit`
  - Apply fit mode only if aspect exceeds threshold (high: 1.6/1.05, low: 1.375/1.275)
  - If within threshold: direct resize to screenshot area dimensions
  - Scaling: use configured method (sharp/smooth)
  - Force alpha channel to 255 (fully opaque)
- [ ] **Sample frame color** from processed screenshot
- [ ] **Draw frame** (rounded rectangle):
  - Upper/lower rectangles: xPos+2 to xPos+width-2, yPos-frameWidth to yPos+height+frameWidth-1
  - Left/right rectangles: xPos-frameWidth to xPos+width+frameWidth-1, yPos+2 to yPos+height-2
  - Four corner circles: radius=8*mult, offset=2*mult from each corner
  - Fill color = sampled frame color
- [ ] **Draw screenshot** on top of frame at calculated position
- [ ] **Process + draw marquee** (if available + enabled):
  - Remove transparent padding
  - Dynamic sizing: widthModifier = clamp(0.5 + widthRatio/6.5, 0, 1), hack for wide logos
  - Resize with Lanczos3 (always, regardless of screenshot scaling setting)
  - Add drop shadow (6px, 0.6 opacity, 4 iterations)
  - Position: top-right (x = canvasWidth - marqueeWidth, y = 0)
  - Alpha-composite onto canvas
- [ ] **Process + draw box** (if available + enabled):
  - Remove transparent padding
  - If aspect ratio > 1.14 and `rotate_horizontal_boxes`: rotate 90 degrees
  - Scale to fit box target dimensions (width-constrained for covers, height-constrained for 3D boxes)
  - Resize with Lanczos3
  - Add drop shadow (6px, 0.6 opacity, 4 iterations)
  - Position: bottom-left (x = 0, y = canvasHeight - boxHeight)
  - Alpha-composite onto canvas
  - Fallback: if no 3D box and `cover_fallback`, use 2D cover with cover target width
- [ ] **Process + draw physical media** (if available + enabled):
  - Remove transparent padding
  - Scale to fit within target dimensions (min of scaleX/scaleY)
  - Resize with Lanczos3
  - Add drop shadow (6px, 0.6 opacity, 4 iterations)
  - Position: right of box (x = xPosBox + boxWidth + 16*mult, y = canvasHeight - mediaHeight)
  - Alpha-composite onto canvas
- [ ] **Save** as PNG or JPG based on config

### 3.4 — Integrate into scrape pipeline
- [ ] After downloading media for a game, if `miximage.enabled`:
  - Check if screenshot exists (required, skip miximage if missing)
  - Check if miximage already exists and `overwrite_existing = false` → skip
  - Locate downloaded media files (screenshot, marquee, box, physical media)
  - Call `MiximageGenerator::generate()`
  - Save to `<media_dir>/<system>/miximages/<rom_stem>.png`
- [ ] Run miximage generation in `spawn_blocking` (CPU-bound image processing)

### 3.5 — Offline generation command (`scrauper generate-miximages`)
- [ ] Scan ES-DE media directories for existing screenshots per system
- [ ] For each screenshot found, look for corresponding marquee, box art, physical media
- [ ] Generate miximage if: screenshot exists AND (no existing miximage OR `overwrite_existing`)
- [ ] Progress bar: "Generating miximages [system]: [X/Y]"
- [ ] No API calls — purely local operation

### 3.6 — Update completeness checker
- [ ] If `miximage.enabled`, check for miximage file in completeness check

## Verification
- [ ] `scrauper scrape --system snes` generates miximages alongside regular media
- [ ] Generated miximages visually match ES-DE's output (compare side by side)
- [ ] All resolution tiers produce correct output (640x480, 1280x960, 1920x1440)
- [ ] Box sizes (small/medium/large) produce correct dimensions
- [ ] Physical media sizes (small/medium/large) produce correct dimensions
- [ ] Drop shadows render correctly on all overlays
- [ ] Frame color sampling produces visually pleasing borders
- [ ] Rounded corners on frame render cleanly
- [ ] Pillarbox/letterbox removal works on screenshots with black bars
- [ ] `scrauper generate-miximages` works offline from existing media
- [ ] Marquee dynamic sizing handles both wide and tall logos correctly
- [ ] Cover fallback works when 3D box is missing
- [ ] Horizontal box rotation works for landscape-oriented box art
- [ ] PNG and JPG output both work
