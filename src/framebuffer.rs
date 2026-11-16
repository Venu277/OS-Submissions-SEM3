//! VenOS Graphical Framebuffer Text Console Subsystem
//!
//! Renders text and graphical diagnostics directly onto the UEFI GOP (Graphics Output
//! Protocol) framebuffer using the anti-aliased `noto_sans_mono_bitmap` font.
//!
//! Provides automatic line wrapping, vertical hardware scrolling, cursor tracking,
//! color attributes, and thread-safe locking with dual-output serial emission.

use bootloader_api::info::{FrameBuffer, FrameBufferInfo, PixelFormat};
use core::fmt;
use noto_sans_mono_bitmap::{FontWeight, RasterHeight, RasterizedChar};
use spin::Mutex;

/// Global thread-safe FrameBufferWriter instance.
pub static FRAMEBUFFER_WRITER: Mutex<Option<FrameBufferWriter>> = Mutex::new(None);

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
}

impl FrameBufferWriter {
    /// Create a new FrameBufferWriter covering the provided UEFI framebuffer.
    pub fn new(framebuffer: &'static mut [u8], info: FrameBufferInfo) -> Self {
        let sample = noto_sans_mono_bitmap::get_raster('A', FontWeight::Regular, RasterHeight::Size16)
            .expect("Font glyph 'A' missing");
        let font_height = sample.height();
        let font_width = sample.width();

        let mut writer = FrameBufferWriter {
            framebuffer,
            info,
            x_pos: 0,
            y_pos: 0,
            text_color: Color::WHITE,
            bg_color: Color::BLACK,
            font_height,
            font_width,
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
                self.write_pixel(x, y, bg);
            }
        }
    }

    /// Retrieve screen resolution dimensions (width, height, stride).
    pub fn dimensions(&self) -> (usize, usize, usize) {
        (
            self.info.width,
            self.info.height,
            self.info.stride,
        )
    }

    /// Set foreground text color.
    pub fn set_text_color(&mut self, color: Color) {
        self.text_color = color;
    }

    /// Set background color.
    pub fn set_bg_color(&mut self, color: Color) {
        self.bg_color = color;
    }

    /// Write a single character to the console.
    pub fn write_char(&mut self, c: char) {
        match c {
            '\n' => self.new_line(),
            '\r' => self.carriage_return(),
            '\x08' => self.backspace(),
            other => {
                if self.x_pos + self.font_width > self.info.width {
                    self.new_line();
                }
                if self.y_pos + self.font_height > self.info.height {
                    self.scroll_up();
                }

                let rendered = noto_sans_mono_bitmap::get_raster(other, FontWeight::Regular, RasterHeight::Size16)
                    .unwrap_or_else(|| {
                        noto_sans_mono_bitmap::get_raster('?', FontWeight::Regular, RasterHeight::Size16)
                            .expect("Fallback glyph missing")
                    });

                self.render_glyph(&rendered);
                self.x_pos += rendered.width();
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
        self.x_pos = 0;
        self.y_pos += self.font_height;
        if self.y_pos + self.font_height > self.info.height {
            self.scroll_up();
        }
    }

    fn backspace(&mut self) {
        if self.x_pos >= self.font_width {
            self.x_pos -= self.font_width;
            let bg = self.bg_color;
            for dy in 0..self.font_height {
                for dx in 0..self.font_width {
                    self.write_pixel(self.x_pos + dx, self.y_pos + dy, bg);
                }
            }
        }
    }

    /// Scroll display content upwards by one line height and blank the exposed bottom line.
    fn scroll_up(&mut self) {
        let bpp = self.info.bytes_per_pixel;
        let stride = self.info.stride;
        let line_bytes = stride * bpp;
        let scroll_bytes = self.font_height * line_bytes;
        let total_bytes = self.info.height * line_bytes;

        if total_bytes > scroll_bytes {
            // Move framebuffer memory up by one line height
            self.framebuffer.copy_within(scroll_bytes..total_bytes, 0);

            // Clear bottom region with background color
            let start_y = self.info.height.saturating_sub(self.font_height);
            let width = self.info.width;
            let bg = self.bg_color;
            for y in start_y..self.info.height {
                for x in 0..width {
                    self.write_pixel(x, y, bg);
                }
            }
        }

        self.y_pos = self.info.height.saturating_sub(self.font_height);
    }

    /// Render a single rasterized character bitmap into the framebuffer.
    fn render_glyph(&mut self, rendered: &RasterizedChar) {
        for (row_idx, row) in rendered.raster().iter().enumerate() {
            let py = self.y_pos + row_idx;
            for (col_idx, &intensity) in row.iter().enumerate() {
                let px = self.x_pos + col_idx;
                if intensity > 0 {
                    // Blend text color with background based on alpha intensity
                    let blended = blend_color(self.text_color, self.bg_color, intensity);
                    self.write_pixel(px, py, blended);
                } else {
                    self.write_pixel(px, py, self.bg_color);
                }
            }
        }
    }

    /// Set an individual pixel in the framebuffer according to the active PixelFormat.
    fn write_pixel(&mut self, x: usize, y: usize, color: Color) {
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
                // Grayscale luminance
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
