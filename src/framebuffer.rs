//! VenOS Graphical Framebuffer Text Console Subsystem
//!
//! Renders text and graphical diagnostics directly onto the UEFI GOP (Graphics Output
//! Protocol) framebuffer using the anti-aliased `noto_sans_mono_bitmap` font.
//!
//! Provides automatic line wrapping, vertical hardware scrolling, cursor tracking,
//! resolution adaptation (aspect ratio, columns/rows), font scaling, color attributes,
//! and thread-safe locking with dual-output serial emission.

use bootloader_api::info::{FrameBuffer, FrameBufferInfo, PixelFormat};
use core::fmt;
use noto_sans_mono_bitmap::{FontWeight, RasterHeight, RasterizedChar};
use spin::Mutex;

/// Global thread-safe FrameBufferWriter instance.
pub static FRAMEBUFFER_WRITER: Mutex<Option<FrameBufferWriter>> = Mutex::new(None);

/// Screen aspect ratio classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AspectRatio {
    Ratio16_9,
    Ratio16_10,
    Ratio4_3,
    Ratio5_4,
    Ratio3_2,
    Other(usize, usize),
}

impl fmt::Display for AspectRatio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AspectRatio::Ratio16_9 => write!(f, "16:9 (Widescreen)"),
            AspectRatio::Ratio16_10 => write!(f, "16:10 (Productivity Widescreen)"),
            AspectRatio::Ratio4_3 => write!(f, "4:3 (Standard)"),
            AspectRatio::Ratio5_4 => write!(f, "5:4 (Standard)"),
            AspectRatio::Ratio3_2 => write!(f, "3:2 (Surface / Laptop)"),
            AspectRatio::Other(w, h) => write!(f, "{}:{} (Custom)", w, h),
        }
    }
}

/// Calculate greatest common divisor using Euclidean algorithm.
pub fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    if a == 0 { 1 } else { a }
}

/// Derive screen aspect ratio from width and height dimensions.
pub fn calculate_aspect_ratio(width: usize, height: usize) -> AspectRatio {
    if height == 0 {
        return AspectRatio::Other(0, 0);
    }
    let d = gcd(width, height);
    let rw = width / d;
    let rh = height / d;
    match (rw, rh) {
        (16, 9) => AspectRatio::Ratio16_9,
        (16, 10) | (8, 5) => AspectRatio::Ratio16_10,
        (4, 3) => AspectRatio::Ratio4_3,
        (5, 4) => AspectRatio::Ratio5_4,
        (3, 2) => AspectRatio::Ratio3_2,
        _ => {
            // Integer ratio heuristic for non-exact resolutions (e.g. 1366x768)
            let r100 = (width * 100) / height;
            if (175..=180).contains(&r100) {
                AspectRatio::Ratio16_9
            } else if (158..=162).contains(&r100) {
                AspectRatio::Ratio16_10
            } else if (131..=136).contains(&r100) {
                AspectRatio::Ratio4_3
            } else {
                AspectRatio::Other(rw, rh)
            }
        }
    }
}

/// RGB 24-bit color representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0 };
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255 };
    pub const LIGHT_GRAY: Color = Color { r: 200, g: 200, b: 200 };
    pub const DARK_GRAY: Color = Color { r: 60, g: 60, b: 60 };
    pub const GREEN: Color = Color { r: 0, g: 255, b: 128 };
    pub const CYAN: Color = Color { r: 0, g: 220, b: 255 };
    pub const YELLOW: Color = Color { r: 255, g: 220, b: 0 };
    pub const RED: Color = Color { r: 255, g: 60, b: 60 };
    pub const VENOS_BLUE: Color = Color { r: 20, g: 40, b: 70 };
}

/// Software text console writer rendering into a linear UEFI GOP framebuffer.
pub struct FrameBufferWriter {
    framebuffer: &'static mut [u8],
    info: FrameBufferInfo,
    x_pos: usize,
    y_pos: usize,
    text_color: Color,
    bg_color: Color,
    font_height: usize,
    font_width: usize,
    font_scale: usize,
    aspect_ratio: AspectRatio,
}

impl FrameBufferWriter {
    /// Create a new FrameBufferWriter covering the provided UEFI framebuffer.
    pub fn new(framebuffer: &'static mut [u8], info: FrameBufferInfo) -> Self {
        let sample = noto_sans_mono_bitmap::get_raster('A', FontWeight::Regular, RasterHeight::Size16)
            .expect("Font glyph 'A' missing");
        let font_height = sample.height();
        let font_width = sample.width();
        let aspect_ratio = calculate_aspect_ratio(info.width, info.height);

        // Auto-scale font: use 1x scaling by default; allow 2x for ultra HiDPI (e.g. 4K >= 3840)
        let font_scale = if info.width >= 3840 && info.height >= 2160 {
            2
        } else {
            1
        };

        let mut writer = FrameBufferWriter {
            framebuffer,
            info,
            x_pos: 0,
            y_pos: 0,
            text_color: Color::WHITE,
            bg_color: Color::BLACK,
            font_height,
            font_width,
            font_scale,
            aspect_ratio,
        };
        writer.clear();
        writer
    }

    /// Clear the entire screen with the current background color.
    pub fn clear(&mut self) {
        self.x_pos = 0;
        self.y_pos = 0;
        let bg = self.bg_color;
        let width = self.info.width;
        let height = self.info.height;
        for y in 0..height {
            for x in 0..width {
                self.write_raw_pixel(x, y, bg);
            }
        }
    }

    /// Retrieve screen resolution dimensions (width, height, stride).
    pub fn dimensions(&self) -> (usize, usize, usize) {
        (self.info.width, self.info.height, self.info.stride)
    }

    /// Retrieve calculated aspect ratio.
    pub fn aspect_ratio(&self) -> AspectRatio {
        self.aspect_ratio
    }

    /// Retrieve console text grid size (columns, rows).
    pub fn grid_size(&self) -> (usize, usize) {
        let eff_w = self.font_width * self.font_scale;
        let eff_h = self.font_height * self.font_scale;
        let cols = if eff_w > 0 { self.info.width / eff_w } else { 0 };
        let rows = if eff_h > 0 { self.info.height / eff_h } else { 0 };
        (cols, rows)
    }

    /// Retrieve current font scale multiplier.
    pub fn font_scale(&self) -> usize {
        self.font_scale
    }

    /// Set font scale factor (e.g. 1 for standard, 2 for HiDPI double-scale).
    pub fn set_font_scale(&mut self, scale: usize) {
        self.font_scale = scale.clamp(1, 4);
    }

    /// Retrieve pixel format description.
    pub fn pixel_format_name(&self) -> &'static str {
        match self.info.pixel_format {
            PixelFormat::Rgb => "RGB (24/32-bit)",
            PixelFormat::Bgr => "BGR (24/32-bit)",
            PixelFormat::U8 => "Grayscale (8-bit)",
            PixelFormat::Unknown { .. } => "Custom / Unknown",
            _ => "Unknown",
        }
    }

    /// Set foreground text color.
    pub fn set_text_color(&mut self, color: Color) {
        self.text_color = color;
    }

    /// Set background color.
    pub fn set_bg_color(&mut self, color: Color) {
        self.bg_color = color;
    }

    /// Write a single character to the console with wrapping and scrolling.
    pub fn write_char(&mut self, c: char) {
        let eff_width = self.font_width * self.font_scale;
        let eff_height = self.font_height * self.font_scale;

        match c {
            '\n' => self.new_line(),
            '\r' => self.carriage_return(),
            '\x08' => self.backspace(),
            other => {
                if self.x_pos + eff_width > self.info.width {
                    self.new_line();
                }
                if self.y_pos + eff_height > self.info.height {
                    self.scroll_up();
                }

                let rendered = noto_sans_mono_bitmap::get_raster(other, FontWeight::Regular, RasterHeight::Size16)
                    .unwrap_or_else(|| {
                        noto_sans_mono_bitmap::get_raster('?', FontWeight::Regular, RasterHeight::Size16)
                            .expect("Fallback glyph missing")
                    });

                self.render_glyph(&rendered);
                self.x_pos += rendered.width() * self.font_scale;
            }
        }
    }

    /// Write an ASCII / UTF-8 string to the console.
    pub fn write_str(&mut self, s: &str) {
        for c in s.chars() {
            self.write_char(c);
        }
    }

    fn carriage_return(&mut self) {
        self.x_pos = 0;
    }

    fn new_line(&mut self) {
        let eff_height = self.font_height * self.font_scale;
        self.x_pos = 0;
        self.y_pos += eff_height;
        if self.y_pos + eff_height > self.info.height {
            self.scroll_up();
        }
    }

    fn backspace(&mut self) {
        let eff_width = self.font_width * self.font_scale;
        let eff_height = self.font_height * self.font_scale;
        if self.x_pos >= eff_width {
            self.x_pos -= eff_width;
            let bg = self.bg_color;
            for dy in 0..eff_height {
                for dx in 0..eff_width {
                    self.write_raw_pixel(self.x_pos + dx, self.y_pos + dy, bg);
                }
            }
        }
    }

    /// Scroll display content upwards by one line height and blank the exposed bottom line.
    fn scroll_up(&mut self) {
        let eff_height = self.font_height * self.font_scale;
        let bpp = self.info.bytes_per_pixel;
        let stride = self.info.stride;
        let line_bytes = stride * bpp;
        let scroll_bytes = eff_height * line_bytes;
        let total_bytes = self.info.height * line_bytes;

        if total_bytes > scroll_bytes {
            // Move framebuffer memory up by one line height
            self.framebuffer.copy_within(scroll_bytes..total_bytes, 0);

            // Clear bottom region with background color
            let start_y = self.info.height.saturating_sub(eff_height);
            let width = self.info.width;
            let bg = self.bg_color;
            for y in start_y..self.info.height {
                for x in 0..width {
                    self.write_raw_pixel(x, y, bg);
                }
            }
        }

        self.y_pos = self.info.height.saturating_sub(eff_height);
    }

    /// Render a single rasterized character bitmap into the framebuffer with scaling.
    fn render_glyph(&mut self, rendered: &RasterizedChar) {
        let scale = self.font_scale;
        for (row_idx, row) in rendered.raster().iter().enumerate() {
            let base_y = self.y_pos + row_idx * scale;
            for (col_idx, &intensity) in row.iter().enumerate() {
                let base_x = self.x_pos + col_idx * scale;
                let color = if intensity > 0 {
                    blend_color(self.text_color, self.bg_color, intensity)
                } else {
                    self.bg_color
                };

                // Draw scaled pixel square
                for dy in 0..scale {
                    for dx in 0..scale {
                        self.write_raw_pixel(base_x + dx, base_y + dy, color);
                    }
                }
            }
        }
    }

    /// Set an individual pixel in the framebuffer according to the active PixelFormat.
    fn write_raw_pixel(&mut self, x: usize, y: usize, color: Color) {
        if x >= self.info.width || y >= self.info.height {
            return;
        }

        let pixel_offset = y * self.info.stride + x;
        let byte_offset = pixel_offset * self.info.bytes_per_pixel;

        if byte_offset + self.info.bytes_per_pixel > self.framebuffer.len() {
            return;
        }

        match self.info.pixel_format {
            PixelFormat::Rgb => {
                self.framebuffer[byte_offset] = color.r;
                self.framebuffer[byte_offset + 1] = color.g;
                self.framebuffer[byte_offset + 2] = color.b;
            }
            PixelFormat::Bgr => {
                self.framebuffer[byte_offset] = color.b;
                self.framebuffer[byte_offset + 1] = color.g;
                self.framebuffer[byte_offset + 2] = color.r;
            }
            PixelFormat::U8 => {
                let gray = ((color.r as u16 * 77 + color.g as u16 * 150 + color.b as u16 * 29) >> 8) as u8;
                self.framebuffer[byte_offset] = gray;
            }
            _ => {}
        }
    }
}

/// Linear alpha blend between foreground and background.
fn blend_color(fg: Color, bg: Color, alpha: u8) -> Color {
    let a = alpha as u32;
    let inv_a = 255 - a;
    Color {
        r: (((fg.r as u32 * a) + (bg.r as u32 * inv_a)) / 255) as u8,
        g: (((fg.g as u32 * a) + (bg.g as u32 * inv_a)) / 255) as u8,
        b: (((fg.b as u32 * a) + (bg.b as u32 * inv_a)) / 255) as u8,
    }
}

impl fmt::Write for FrameBufferWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_str(s);
        Ok(())
    }
}

/// Initialize the global FrameBufferWriter singleton from the UEFI BootInfo framebuffer.
pub fn init_framebuffer(framebuffer: &'static mut FrameBuffer) {
    let info = framebuffer.info();
    let buffer = framebuffer.buffer_mut();
    let writer = FrameBufferWriter::new(buffer, info);
    *FRAMEBUFFER_WRITER.lock() = Some(writer);
}

/// Query current display summary: (width, height, stride, aspect_ratio, cols, rows, font_scale, format_name).
pub fn display_summary() -> Option<(usize, usize, usize, AspectRatio, usize, usize, usize, &'static str)> {
    let lock = FRAMEBUFFER_WRITER.lock();
    lock.as_ref().map(|w| {
        let (width, height, stride) = w.dimensions();
        let (cols, rows) = w.grid_size();
        (
            width,
            height,
            stride,
            w.aspect_ratio(),
            cols,
            rows,
            w.font_scale(),
            w.pixel_format_name(),
        )
    })
}

/// Print formatted text to both the UEFI Framebuffer console and the UART serial port.
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        if let Some(ref mut writer) = *FRAMEBUFFER_WRITER.lock() {
            let _ = writer.write_fmt(args);
        }
    });

    // Also mirror to serial port for automated analysis and headless logging
    crate::serial::_print(args);
}

/// Like the standard `print!` macro in std, printing to the graphical console & serial.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::framebuffer::_print(format_args!($($arg)*))
    };
}

/// Like the standard `println!` macro in std, printing to the graphical console & serial.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}
