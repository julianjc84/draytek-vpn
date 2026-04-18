/// Colored VPN shield icons as ARGB32 pixmaps for the system tray.
use std::sync::LazyLock;

const SIZE: i32 = 22;

/// Connected — green shield with checkmark
pub static CONNECTED: LazyLock<ksni::Icon> =
    LazyLock::new(|| shield_icon(0x4C, 0xAF, 0x50, Glyph::Check));

/// Disconnected — red shield with X
pub static DISCONNECTED: LazyLock<ksni::Icon> =
    LazyLock::new(|| shield_icon(0xF4, 0x43, 0x36, Glyph::Cross));

/// Connecting/disconnecting — amber shield with dots
pub static CONNECTING: LazyLock<ksni::Icon> =
    LazyLock::new(|| shield_icon(0xFF, 0x98, 0x00, Glyph::Dots));

enum Glyph {
    Check,
    Cross,
    Dots,
}

fn shield_icon(r: u8, g: u8, b: u8, glyph: Glyph) -> ksni::Icon {
    let s = SIZE as usize;
    let mut data = vec![0u8; s * s * 4];

    // Draw shield shape
    for y in 0..s {
        for x in 0..s {
            if is_shield(x, y, s) {
                set_pixel(&mut data, x, y, s, [255, r, g, b]);
            }
        }
    }

    // Draw white glyph on top
    match glyph {
        Glyph::Check => draw_check(&mut data, s),
        Glyph::Cross => draw_cross(&mut data, s),
        Glyph::Dots => draw_dots(&mut data, s),
    }

    ksni::Icon {
        width: SIZE,
        height: SIZE,
        data,
    }
}

/// Shield shape: wide at top, narrows to a point at bottom.
fn is_shield(x: usize, y: usize, s: usize) -> bool {
    // Shield spans y=2..19 (for s=22)
    let top = s * 2 / 22;
    let bottom = s * 19 / 22;
    if y < top || y > bottom {
        return false;
    }

    let cx = s as f32 / 2.0;
    let fx = x as f32 + 0.5;
    let fy = y as f32 + 0.5;

    // Top half: rounded rectangle, bottom half: narrows to point
    let mid_y = s as f32 * 10.0 / 22.0;
    let half_width = if fy < mid_y {
        // Top: constant width
        s as f32 * 8.0 / 22.0
    } else {
        // Bottom: linear taper to point
        let progress = (fy - mid_y) / (bottom as f32 - mid_y);
        s as f32 * 8.0 / 22.0 * (1.0 - progress)
    };

    (fx - cx).abs() <= half_width
}

fn set_pixel(data: &mut [u8], x: usize, y: usize, stride: usize, argb: [u8; 4]) {
    let idx = (y * stride + x) * 4;
    if idx + 3 < data.len() {
        data[idx..idx + 4].copy_from_slice(&argb);
    }
}

/// Draw a white checkmark.
fn draw_check(data: &mut [u8], s: usize) {
    // Checkmark: from (5,11) down to (9,15) then up to (16,8)
    // Scaled to icon size
    let points: &[(f32, f32, f32, f32)] = &[
        (5.0, 11.0, 9.0, 15.0), // descending stroke
        (9.0, 15.0, 16.0, 8.0), // ascending stroke
    ];
    for &(x1, y1, x2, y2) in points {
        draw_thick_line(
            data,
            s,
            (x1 * s as f32 / 22.0) as i32,
            (y1 * s as f32 / 22.0) as i32,
            (x2 * s as f32 / 22.0) as i32,
            (y2 * s as f32 / 22.0) as i32,
        );
    }
}

/// Draw a white X.
fn draw_cross(data: &mut [u8], s: usize) {
    let points: &[(f32, f32, f32, f32)] = &[(7.0, 8.0, 15.0, 16.0), (15.0, 8.0, 7.0, 16.0)];
    for &(x1, y1, x2, y2) in points {
        draw_thick_line(
            data,
            s,
            (x1 * s as f32 / 22.0) as i32,
            (y1 * s as f32 / 22.0) as i32,
            (x2 * s as f32 / 22.0) as i32,
            (y2 * s as f32 / 22.0) as i32,
        );
    }
}

/// Draw three white dots (ellipsis).
fn draw_dots(data: &mut [u8], s: usize) {
    let cy = s * 12 / 22;
    for &cx_frac in &[8.0, 11.0, 14.0] {
        let cx = (cx_frac * s as f32 / 22.0) as usize;
        // 2x2 dot
        for dy in 0..2usize {
            for dx in 0..2usize {
                let px = cx + dx;
                let py = cy + dy;
                if px < s && py < s {
                    set_pixel(data, px, py, s, [255, 255, 255, 255]);
                }
            }
        }
    }
}

/// Draw a thick (2px) white line using Bresenham's algorithm.
fn draw_thick_line(data: &mut [u8], s: usize, x1: i32, y1: i32, x2: i32, y2: i32) {
    let dx = (x2 - x1).abs();
    let dy = -(y2 - y1).abs();
    let sx: i32 = if x1 < x2 { 1 } else { -1 };
    let sy: i32 = if y1 < y2 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut x = x1;
    let mut y = y1;

    loop {
        // Draw 2x2 block for thickness
        for oy in 0..2i32 {
            for ox in 0..2i32 {
                let px = (x + ox) as usize;
                let py = (y + oy) as usize;
                if px < s && py < s {
                    set_pixel(data, px, py, s, [255, 255, 255, 255]);
                }
            }
        }

        if x == x2 && y == y2 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}
