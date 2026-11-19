use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use spin::Mutex;

static KEYBOARD_WAKER: Mutex<Option<Waker>> = Mutex::new(None);

/// Called by the keyboard ISR or timer ISR when new input is available.
pub(crate) fn wake_keyboard_task() {
    x86_64::instructions::interrupts::without_interrupts(|| {
        if let Some(waker) = KEYBOARD_WAKER.lock().take() {
            waker.wake();
        }
    });
}

/// Unified console input stream combining PS/2 keyboard and UART serial port.
pub struct InputStream {
    keyboard: pc_keyboard::Keyboard<pc_keyboard::layouts::Us104Key, pc_keyboard::ScancodeSet1>,
}

impl InputStream {
    pub fn new() -> Self {
        InputStream {
            keyboard: pc_keyboard::Keyboard::new(
                pc_keyboard::ScancodeSet1::new(),
                pc_keyboard::layouts::Us104Key,
                pc_keyboard::HandleControl::Ignore,
            ),
        }
    }

    /// Wait for the next decoded character from either keyboard or serial port asynchronously.
    pub async fn next_char(&mut self) -> char {
        struct NextChar<'a>(&'a mut InputStream);

        impl<'a> Future for NextChar<'a> {
            type Output = char;

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                // 1. Check UART serial port input first
                if let Some(byte) = crate::serial::poll_serial_byte() {
                    let c = match byte {
                        b'\r' | b'\n' => Some('\n'),
                        8 | 0x7F => Some('\x08'),
                        ascii if (0x20..=0x7E).contains(&ascii) => Some(ascii as char),
                        _ => None,
                    };
                    if let Some(c) = c {
                        return Poll::Ready(c);
                    }
                }

                // 2. Check PS/2 keyboard scancodes
                while let Some(scancode) = crate::interrupts::pop_scancode() {
                    if let Ok(Some(key_event)) = self.0.keyboard.add_byte(scancode) {
                        if let Some(key) = self.0.keyboard.process_keyevent(key_event) {
                            if let pc_keyboard::DecodedKey::Unicode(c) = key {
                                return Poll::Ready(c);
                            }
                        }
                    }
                }

                // 3. Register waker before sleeping
                let mut waker_guard = KEYBOARD_WAKER.lock();
                *waker_guard = Some(cx.waker().clone());

                // 4. Re-check to eliminate race condition
                if let Some(byte) = crate::serial::poll_serial_byte() {
                    let c = match byte {
                        b'\r' | b'\n' => Some('\n'),
                        8 | 0x7F => Some('\x08'),
                        ascii if (0x20..=0x7E).contains(&ascii) => Some(ascii as char),
                        _ => None,
                    };
                    if let Some(c) = c {
                        *waker_guard = None;
                        return Poll::Ready(c);
                    }
                }

                while let Some(scancode) = crate::interrupts::pop_scancode() {
                    if let Ok(Some(key_event)) = self.0.keyboard.add_byte(scancode) {
                        if let Some(key) = self.0.keyboard.process_keyevent(key_event) {
                            if let pc_keyboard::DecodedKey::Unicode(c) = key {
                                *waker_guard = None;
                                return Poll::Ready(c);
                            }
                        }
                    }
                }

                Poll::Pending
            }
        }

        NextChar(self).await
    }
}

pub struct ScancodeStream {
    _private: (),
}

impl ScancodeStream {
    pub fn new() -> Self {
        ScancodeStream { _private: () }
    }

    /// Wait for the next scancode asynchronously without busy waiting.
    pub async fn next_scancode(&self) -> u8 {
        struct NextScancode;
        impl Future for NextScancode {
            type Output = u8;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                if let Some(scancode) = crate::interrupts::pop_scancode() {
                    Poll::Ready(scancode)
                } else {
                    let mut waker_guard = KEYBOARD_WAKER.lock();
                    *waker_guard = Some(cx.waker().clone());
                    if let Some(scancode) = crate::interrupts::pop_scancode() {
                        *waker_guard = None;
                        Poll::Ready(scancode)
                    } else {
                        Poll::Pending
                    }
                }
            }
        }
        NextScancode.await
    }
}
