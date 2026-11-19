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

/// Probe whether a physical 16550 UART controller is present at base I/O port 0x3F8.
///
/// Modern laptops (including HP Pavilion 15) do not have a physical COM1 UART.
/// Unmapped I/O port reads return 0xFF on x86 due to floating bus lines.
/// This probe uses the standard 16550 scratchpad register test (offset +7, port 0x3FF)
/// to verify that hardware registers can genuinely retain test bit patterns (0x55, 0xAA).
pub fn is_serial_available() -> bool {
    use x86_64::instructions::port::Port;
    let mut scratch: Port<u8> = Port::new(0x3FF); // Base (0x3F8) + 7: Scratchpad Register
    unsafe {
        scratch.write(0x55);
        if scratch.read() != 0x55 {
            return false;
        }
        scratch.write(0xAA);
        if scratch.read() != 0xAA {
            return false;
        }
        true
    }
}

/// Poll a received byte from the UART 16550 serial port without blocking.
/// Returns `Some(byte)` if data is waiting in the receiver buffer and hardware is present, or `None`.
pub fn poll_serial_byte() -> Option<u8> {
    if !is_serial_available() {
        return None;
    }
    use x86_64::instructions::port::PortReadOnly;
    let mut line_sts: PortReadOnly<u8> = PortReadOnly::new(0x3FD); // LSR: Line Status Register (0x3F8 + 5)
    let mut data: PortReadOnly<u8> = PortReadOnly::new(0x3F8);    // RBR: Receiver Buffer Register
    unsafe {
        let status = line_sts.read();
        // 0xFF indicates floating/disconnected bus
        if status != 0xFF && (status & 0x01) != 0 {
            let byte = data.read();
            if byte != 0xFF && byte != 0x00 {
                Some(byte)
            } else {
                None
            }
        } else {
            None
        }
    }
}
