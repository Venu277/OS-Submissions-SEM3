use lazy_static::lazy_static;
use spin::Mutex;
use uart_16550::SerialPort;

lazy_static! {
    pub static ref SERIAL1: Mutex<SerialPort> = {
        let mut serial_port = unsafe { SerialPort::new(0x3F8) };
        serial_port.init();
        Mutex::new(serial_port)
    };
}

#[doc(hidden)]
pub fn _print(args: core::fmt::Arguments) {
    use core::fmt::Write;
    x86_64::instructions::interrupts::without_interrupts(|| {
        SERIAL1
            .lock()
            .write_fmt(args)
            .expect("Printing to serial failed");
    });
}

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => ($crate::serial::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($($arg:tt)*) => ($crate::serial_print!("{}\n", format_args!($($arg)*)));
}

/// Poll a received byte from the UART 16550 serial port without blocking.
/// Returns `Some(byte)` if data is waiting in the receiver buffer, or `None`.
pub fn poll_serial_byte() -> Option<u8> {
    use x86_64::instructions::port::PortReadOnly;
    let mut line_sts: PortReadOnly<u8> = PortReadOnly::new(0x3FD); // LSR: Line Status Register (0x3F8 + 5)
    let mut data: PortReadOnly<u8> = PortReadOnly::new(0x3F8);    // RBR: Receiver Buffer Register
    unsafe {
        if (line_sts.read() & 0x01) != 0 {
            Some(data.read())
        } else {
            None
        }
    }
}
