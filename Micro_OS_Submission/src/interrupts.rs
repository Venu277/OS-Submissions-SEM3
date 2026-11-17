use crate::gdt;
use crate::println;
use core::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering};
use lazy_static::lazy_static;
use pic8259::ChainedPics;
use spin;
use x86_64::instructions::port::Port;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

// ---------- PIC 8259 setup ----------

/// PIC1 starts at interrupt vector 32 (right after CPU exceptions 0-31).
pub const PIC_1_OFFSET: u8 = 32;
/// PIC2 starts at interrupt vector 40.
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: spin::Mutex<ChainedPics> =
    spin::Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

/// Hardware interrupt vector numbers (after PIC remapping).
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,        // IRQ 0 → vector 32
    Keyboard = PIC_1_OFFSET + 1, // IRQ 1 → vector 33
    PrimaryAta = PIC_1_OFFSET + 14,   // IRQ 14 → vector 46
    SecondaryAta = PIC_1_OFFSET + 15, // IRQ 15 → vector 47
}

impl InterruptIndex {
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

// ---------- Lock-Free Scancode Queue (SPSC) ----------

const SCANCODE_QUEUE_CAPACITY: usize = 128;

pub struct ScancodeQueue {
    buffer: [AtomicU8; SCANCODE_QUEUE_CAPACITY],
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl ScancodeQueue {
    pub const fn new() -> Self {
        const ZERO: AtomicU8 = AtomicU8::new(0);
        Self {
            buffer: [ZERO; SCANCODE_QUEUE_CAPACITY],
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Push a scancode into the queue. Wait-free and lock-free; safe in interrupt context.
    pub fn push(&self, scancode: u8) -> Result<(), ()> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head.wrapping_sub(tail) >= SCANCODE_QUEUE_CAPACITY {
            return Err(()); // Queue is full; drop scancode to avoid deadlock
        }
        self.buffer[head % SCANCODE_QUEUE_CAPACITY].store(scancode, Ordering::Relaxed);
        self.head.store(head.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    /// Pop a scancode from the queue. Non-blocking; called outside interrupt context.
    pub fn pop(&self) -> Option<u8> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if head == tail {
            return None; // Queue is empty
        }
        let scancode = self.buffer[tail % SCANCODE_QUEUE_CAPACITY].load(Ordering::Relaxed);
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        Some(scancode)
    }
}

pub static SCANCODE_QUEUE: ScancodeQueue = ScancodeQueue::new();

/// Pop a pending keyboard scancode from the lock-free queue or hardware port 0x60.
pub fn pop_scancode() -> Option<u8> {
    if let Some(scancode) = SCANCODE_QUEUE.pop() {
        Some(scancode)
    } else {
        poll_hardware_scancode()
    }
}

/// Direct polling of i8042 data port 0x60.
/// Returns `Some(scancode)` if output buffer is full (status bit 0 set) and data is from the keyboard, else `None`.
pub fn poll_hardware_scancode() -> Option<u8> {
    let mut status_port: Port<u8> = Port::new(0x64);
    let mut data_port: Port<u8> = Port::new(0x60);

    unsafe {
        let status = status_port.read();
        // Bit 0: Output buffer full (data waiting to be read from 0x60)
        if (status & 0x01) != 0 {
            let scancode = data_port.read();
            // Bit 5: 0 = keyboard (first PS/2 port), 1 = auxiliary device (mouse / touchpad)
            if (status & 0x20) == 0 {
                Some(scancode)
            } else {
                None // Discard mouse packet bytes
            }
        } else {
            None
        }
    }
}

/// Initialize i8042 PS/2 controller on bare-metal systems.
///
/// On pure UEFI Class 3 systems (e.g. 13th Gen Intel Core laptops like HP Pavilion 15),
/// UEFI firmware leaves the i8042 controller disabled or unconfigured upon ExitBootServices.
/// This function:
/// 1. Flushes stale bytes from the output buffer.
/// 2. Enables the first PS/2 port (keyboard).
/// 3. Configures the Controller Configuration Byte (CCB) to enable IRQ 1, enable the port clock,
///    and enable automatic hardware translation to Scancode Set 1.
/// 4. Sends command 0xF4 to the keyboard to enable keyboard scanning.
pub fn init_ps2_keyboard() {
    let mut status_cmd: Port<u8> = Port::new(0x64);
    let mut data: Port<u8> = Port::new(0x60);

    unsafe {
        // Step 1: Flush stale bytes from the output buffer (up to 64 bytes)
        for _ in 0..64 {
            if (status_cmd.read() & 0x01) == 0 {
                break;
            }
            let _ = data.read();
        }

        // Step 2: Enable the first PS/2 port (command 0xAE)
        let mut timeout = 10000usize;
        while (status_cmd.read() & 0x02) != 0 && timeout > 0 {
            timeout -= 1;
            core::hint::spin_loop();
        }
        status_cmd.write(0xAE);

        // Step 3: Read Controller Configuration Byte (CCB) via command 0x20
        timeout = 10000;
        while (status_cmd.read() & 0x02) != 0 && timeout > 0 {
            timeout -= 1;
            core::hint::spin_loop();
        }
        status_cmd.write(0x20);

        timeout = 10000;
        while (status_cmd.read() & 0x01) == 0 && timeout > 0 {
            timeout -= 1;
            core::hint::spin_loop();
        }
        let config = if timeout > 0 {
            data.read()
        } else {
            0b01000101 // Default fallback: translation + sysflag + IRQ1
        };

        // Step 4: Modify CCB:
        // - Set bit 0 (0x01): First PS/2 port interrupt enabled
        // - Clear bit 4 (0x10): First PS/2 port clock enabled (0 = enabled)
        // - Set bit 6 (0x40): First PS/2 port translation to Scancode Set 1 enabled
        // - Retain bit 2 (0x04): System flag
        let new_config = (config | 0x01 | 0x40) & !0x10;

        timeout = 10000;
        while (status_cmd.read() & 0x02) != 0 && timeout > 0 {
            timeout -= 1;
            core::hint::spin_loop();
        }
        status_cmd.write(0x60); // Write CCB command

        timeout = 10000;
        while (status_cmd.read() & 0x02) != 0 && timeout > 0 {
            timeout -= 1;
            core::hint::spin_loop();
        }
        data.write(new_config);

        // Step 5: Send Enable Scanning command (0xF4) to the keyboard
        timeout = 10000;
        while (status_cmd.read() & 0x02) != 0 && timeout > 0 {
            timeout -= 1;
            core::hint::spin_loop();
        }
        data.write(0xF4);

        // Wait briefly for ACK (0xFA) and drain it so it doesn't appear as a keypress
        timeout = 10000;
        while (status_cmd.read() & 0x01) == 0 && timeout > 0 {
            timeout -= 1;
            core::hint::spin_loop();
        }
        if timeout > 0 {
            let _ = data.read();
        }
    }
}

// ---------- Timer Tick Accounting ----------

static TIMER_TICKS: AtomicU64 = AtomicU64::new(0);

/// Get the current number of timer interrupt ticks elapsed.
pub fn timer_ticks() -> u64 {
    TIMER_TICKS.load(Ordering::Relaxed)
}

// ---------- PIT Timer Configuration ----------

/// Programmable Interval Timer (PIT) 8253/8254 clock frequency (Hz).
pub const PIT_BASE_FREQUENCY_HZ: u32 = 1_193_182;

/// Configure PIT Channel 0 to generate periodic timer interrupts at `freq_hz`.
///
/// Port 0x43 is the Mode/Command register.
/// Port 0x40 is Channel 0 Data port.
/// Mode 0x34: Channel 0, lobyte/hibyte access, Mode 2 (rate generator), 16-bit binary.
pub fn init_pit(freq_hz: u32) {
    let divisor = (PIT_BASE_FREQUENCY_HZ / freq_hz.max(18)).clamp(1, 65535) as u16;
    let mut pit_cmd: Port<u8> = Port::new(0x43);
    let mut pit_ch0: Port<u8> = Port::new(0x40);

    // SAFETY: Writing valid command byte 0x34 and 16-bit divisor to standard x86 PIT I/O ports
    // 0x43 and 0x40. Does not access unmapped memory.
    unsafe {
        pit_cmd.write(0x34);
        pit_ch0.write((divisor & 0xFF) as u8);
        pit_ch0.write((divisor >> 8) as u8);
    }
}

// ---------- PIT TSC Calibration ----------

/// Calibrates the CPU TSC frequency using PIT Channel 2 one-shot mode over ~10 ms.
/// Returns estimated TSC frequency in Hz.
pub fn calibrate_tsc() -> u64 {
    // PIT operates at 1,193,182 Hz. 10 ms is ~11,932 counter ticks.
    let mut pit_cmd: Port<u8> = Port::new(0x43);
    let mut pit_ch2: Port<u8> = Port::new(0x42);
    let mut port_61: Port<u8> = Port::new(0x61);

    unsafe {
        // Disable Channel 2 gate and speaker output
        let orig = port_61.read();
        port_61.write(orig & !0x03);

        // PIT Mode: Channel 2, Access mode lobyte/hibyte, Mode 0 (interrupt on terminal count)
        // 0b10110000 = 0xB0
        pit_cmd.write(0xB0);

        let count: u16 = 11932; // ~10 ms (11932 / 1193182 = 0.01000015 s)
        pit_ch2.write((count & 0xFF) as u8);
        pit_ch2.write((count >> 8) as u8);

        // Start timer by asserting Channel 2 gate (bit 0 = 1)
        port_61.write((orig & !0x02) | 0x01);

        let tsc_start = core::arch::x86_64::_rdtsc();

        // Spin until Channel 2 OUT (bit 5 of port 0x61) transitions to high
        let mut timeout = 10_000_000usize;
        while (port_61.read() & 0x20) == 0 && timeout > 0 {
            timeout -= 1;
            core::hint::spin_loop();
        }

        let tsc_end = core::arch::x86_64::_rdtsc();

        // Restore port 0x61 state
        port_61.write(orig & !0x03);

        if timeout == 0 {
            // Fallback if PIT Channel 2 didn't respond: default to 2.0 GHz
            return 2_000_000_000;
        }

        let elapsed_tsc = tsc_end.wrapping_sub(tsc_start);
        // Frequency (Hz) = elapsed_tsc / 0.01s = elapsed_tsc * 100
        let freq = elapsed_tsc.saturating_mul(100);
        if freq < 100_000_000 {
            2_000_000_000 // Fallback if unrealistic
        } else {
            freq
        }
    }
}

// ---------- IDT Setup ----------

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        // CPU exceptions
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }
        idt.page_fault.set_handler_fn(page_fault_handler);
        idt.general_protection_fault.set_handler_fn(general_protection_fault_handler);

        // Hardware interrupts (PIC)
        idt[InterruptIndex::Timer.as_usize()].set_handler_fn(timer_interrupt_handler);
        idt[InterruptIndex::Keyboard.as_usize()].set_handler_fn(keyboard_interrupt_handler);
        idt[InterruptIndex::PrimaryAta.as_usize()].set_handler_fn(ata_primary_interrupt_handler);
        idt[InterruptIndex::SecondaryAta.as_usize()].set_handler_fn(ata_secondary_interrupt_handler);

        idt
    };
}

/// Load the Interrupt Descriptor Table into the CPU IDTR.
pub fn init_idt() {
    IDT.load();
}

// ---------- CPU Exception Handlers ----------

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    println!("EXCEPTION: PAGE FAULT");
    println!("Accessed address: {:?}", Cr2::read());
    println!("Error code: {:?}", error_code);
    println!("{:#?}", stack_frame);
    crate::hlt_loop();
}

extern "x86-interrupt" fn general_protection_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    panic!(
        "EXCEPTION: GENERAL PROTECTION FAULT (code {:#x})\n{:#?}",
        error_code, stack_frame
    );
}

// ---------- Hardware Interrupt Handlers ----------

extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }

    // Hardware scancode polling fallback for bare-metal UEFI systems:
    // If PIC IRQ 1 is masked or not routed by laptop UEFI firmware, poll port 0x60 directly.
    for _ in 0..16 {
        match poll_hardware_scancode() {
            Some(scancode) => {
                let _ = SCANCODE_QUEUE.push(scancode);
                crate::task::keyboard::wake_keyboard_task();
            }
            None => break,
        }
    }

    // Wake console input stream if serial byte is waiting
    unsafe {
        let mut line_sts: x86_64::instructions::port::PortReadOnly<u8> =
            x86_64::instructions::port::PortReadOnly::new(0x3FD);
        if (line_sts.read() & 0x01) != 0 {
            crate::task::keyboard::wake_keyboard_task();
        }
    }

    // CA-6 Layer 6: Timer-driven preemptive scheduling
    crate::scheduler::timer_tick();
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    let mut port = Port::new(0x60); // PS/2 data port
    let scancode: u8 = unsafe { port.read() };

    // Push into lock-free queue; non-blocking, deadlock-free
    let _ = SCANCODE_QUEUE.push(scancode);

    // Wake async keyboard executor task if sleeping
    crate::task::keyboard::wake_keyboard_task();

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}

extern "x86-interrupt" fn ata_primary_interrupt_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::PrimaryAta.as_u8());
    }
}

extern "x86-interrupt" fn ata_secondary_interrupt_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::SecondaryAta.as_u8());
    }
}

// ---------- Unit Tests ----------

#[test_case]
fn test_breakpoint_exception() {
    x86_64::instructions::interrupts::int3();
}
