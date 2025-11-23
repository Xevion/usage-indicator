use crate::error::ErrorIndicator;

// Icon rendering configuration
pub const ICON_SIZE: u32 = 32; // Final tray icon size
const RENDER_SCALE: u32 = 4; // Render at 4x for quality
const RENDER_SIZE: u32 = ICON_SIZE * RENDER_SCALE; // 128px

// Font sizes (scaled for render resolution)
const PERCENTAGE_FONT_SIZE: f32 = 124.0; // 31.0 * 4
const UNKNOWN_FONT_SIZE: f32 = 80.0; // 20.0 * 4

// Staleness threshold (30 minutes)
pub const STALENESS_THRESHOLD_SECS: u64 = 1800;

/// Calculate color based on usage percentage with gradient:
/// 0-50%: Green → Yellow
/// 50-100%: Yellow → Red
fn usage_to_color(percentage: u8) -> [u8; 3] {
    let pct = percentage.min(100) as f32 / 100.0;

    // Define color stops
    const GREEN: [f32; 3] = [0.0, 200.0, 83.0]; // #00C853
    const YELLOW: [f32; 3] = [255.0, 214.0, 0.0]; // #FFD600
    const RED: [f32; 3] = [211.0, 47.0, 47.0]; // #D32F2F

    let rgb = if pct <= 0.5 {
        // Interpolate between GREEN and YELLOW (0-50%)
        let t = pct * 2.0; // Normalize to 0-1 range
        [
            GREEN[0] + (YELLOW[0] - GREEN[0]) * t,
            GREEN[1] + (YELLOW[1] - GREEN[1]) * t,
            GREEN[2] + (YELLOW[2] - GREEN[2]) * t,
        ]
    } else {
        // Interpolate between YELLOW and RED (50-100%)
        let t = (pct - 0.5) * 2.0; // Normalize to 0-1 range
        [
            YELLOW[0] + (RED[0] - YELLOW[0]) * t,
            YELLOW[1] + (RED[1] - YELLOW[1]) * t,
            YELLOW[2] + (RED[2] - YELLOW[2]) * t,
        ]
    };

    [rgb[0] as u8, rgb[1] as u8, rgb[2] as u8]
}

/// Calculate relative luminance and return appropriate text color for contrast
/// Returns (r, g, b) where each component is 0 or 255
fn contrast_text_color(bg_rgb: [u8; 3]) -> [u8; 3] {
    // Calculate relative luminance using sRGB formula
    let r = bg_rgb[0] as f32 / 255.0;
    let g = bg_rgb[1] as f32 / 255.0;
    let b = bg_rgb[2] as f32 / 255.0;

    let luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;

    // Use white text on dark backgrounds, black on light backgrounds
    if luminance > 0.5 {
        [0, 0, 0] // Black text
    } else {
        [255, 255, 255] // White text
    }
}

/// Measure text dimensions using ab_glyph metrics
/// Returns (width, height)
fn measure_text_bounds(
    text: &str,
    font: &ab_glyph::FontRef,
    scale: ab_glyph::PxScale,
) -> (f32, f32) {
    use ab_glyph::{Font, ScaleFont};

    let scaled_font = font.as_scaled(scale);

    // Calculate text width by summing glyph advances
    let mut width = 0.0;
    for ch in text.chars() {
        let glyph_id = font.glyph_id(ch);
        width += scaled_font.h_advance(glyph_id);
    }

    // Calculate height from font metrics
    let ascent = scaled_font.ascent();
    let descent = scaled_font.descent();
    let height = ascent - descent;

    (width, height)
}

/// Calculate position to center text on canvas
fn calculate_centered_position(text_width: f32, text_height: f32, canvas_size: u32) -> (i32, i32) {
    let canvas_f = canvas_size as f32;

    // Center horizontally
    let x = ((canvas_f - text_width) / 2.0) as i32;

    // Center vertically
    let y = ((canvas_f - text_height) / 2.0) as i32;

    (x, y)
}

/// Generate icon with usage percentage displayed on color gradient background
pub fn generate_usage_icon(percentage: u8, error_indicator: ErrorIndicator) -> Vec<u8> {
    use ab_glyph::{FontRef, PxScale};
    use image::{Rgba, RgbaImage, imageops};
    use imageproc::drawing::{draw_hollow_rect_mut, draw_text_mut};
    use imageproc::rect::Rect;

    // Get background color based on usage
    let bg_color = usage_to_color(percentage);
    let mut img = RgbaImage::from_pixel(
        RENDER_SIZE,
        RENDER_SIZE,
        Rgba([bg_color[0], bg_color[1], bg_color[2], 255]),
    );

    // Draw error indicator border if needed
    if let Some(border_color) = error_indicator.border_color() {
        let border_rgba = Rgba([border_color[0], border_color[1], border_color[2], 255]);
        let border_width = 8; // Scaled for high-res rendering

        // Draw multiple rectangles to create thick border
        for i in 0..border_width {
            let rect =
                Rect::at(i as i32, i as i32).of_size(RENDER_SIZE - (i * 2), RENDER_SIZE - (i * 2));
            draw_hollow_rect_mut(&mut img, rect, border_rgba);
        }
    }

    // Get contrasting text color
    let text_color = contrast_text_color(bg_color);
    let text_rgba = Rgba([text_color[0], text_color[1], text_color[2], 255]);

    // Load embedded font
    let font_data = include_bytes!("../fonts/Roboto-Bold.ttf");
    let font = FontRef::try_from_slice(font_data).expect("Failed to load font");

    // Format percentage text
    let text = format!("{:2}", percentage);

    // Use scaled font size for high-resolution rendering
    let scale = PxScale::from(PERCENTAGE_FONT_SIZE);

    // Measure text dimensions
    let (text_width, text_height) = measure_text_bounds(&text, &font, scale);

    // Calculate centered position
    let (x, y) = calculate_centered_position(text_width, text_height, RENDER_SIZE);

    // Draw text at calculated position
    draw_text_mut(&mut img, text_rgba, x, y, scale, &font, &text);

    // Downscale to final icon size for better quality
    let final_img = imageops::resize(&img, ICON_SIZE, ICON_SIZE, imageops::FilterType::Lanczos3);

    final_img.into_raw()
}

/// Generate icon with question mark for unknown state
pub fn generate_unknown_icon() -> Vec<u8> {
    use ab_glyph::{FontRef, PxScale};
    use image::{Rgba, RgbaImage, imageops};
    use imageproc::drawing::draw_text_mut;

    // Gray background for unknown state
    let mut img = RgbaImage::from_pixel(RENDER_SIZE, RENDER_SIZE, Rgba([128, 128, 128, 255]));

    // White question mark
    let text_rgba = Rgba([255, 255, 255, 255]);

    // Load embedded font
    let font_data = include_bytes!("../fonts/Roboto-Bold.ttf");
    let font = FontRef::try_from_slice(font_data).expect("Failed to load font");

    // Use scaled font size for high-resolution rendering
    let scale = PxScale::from(UNKNOWN_FONT_SIZE);
    let text = "?";

    // Measure text dimensions
    let (text_width, text_height) = measure_text_bounds(text, &font, scale);

    // Calculate centered position
    let (x, y) = calculate_centered_position(text_width, text_height, RENDER_SIZE);

    // Draw text at calculated position
    draw_text_mut(&mut img, text_rgba, x, y, scale, &font, text);

    // Downscale to final icon size for better quality
    let final_img = imageops::resize(&img, ICON_SIZE, ICON_SIZE, imageops::FilterType::Lanczos3);

    final_img.into_raw()
}
