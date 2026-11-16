//! Low-Level AMD64 Context Switch Assembly Routine
//!
//! Specification: CA-6 Section 9 (Layer 5 - Round Robin Scheduler) & Section 26 (Unsafe Rust Policy)
//!
//! Preserves and restores callee-saved registers and the hardware stack pointer
//! according to the AMD64 System V ABI calling convention.

use crate::kthread::CpuContext;

core::arch::global_asm!(
    r#"
    .global switch_context
    .type switch_context, @function
    switch_context:
        // =====================================================================
        // Step 1: Save outgoing thread execution context
        // Argument 1 (rdi): *mut CpuContext (prev thread)
        // =====================================================================
        mov [rdi + 0x00], r15
        mov [rdi + 0x08], r14
        mov [rdi + 0x10], r13
        mov [rdi + 0x18], r12
        mov [rdi + 0x20], rbx
        mov [rdi + 0x28], rbp

        // Save CPU rflags (arithmetic status, interrupt flag, etc.)
        pushfq
        pop rax
        mov [rdi + 0x38], rax

        // Save current stack pointer (rsp).
        // Note: [rsp] contains the return address pushed by the `call` instruction.
        mov [rdi + 0x30], rsp

        // =====================================================================
        // Step 2: Restore incoming thread execution context
        // Argument 2 (rsi): *const CpuContext (next thread)
        // =====================================================================
        // Switch to the target thread's private stack
        mov rsp, [rsi + 0x30]

        // Restore rflags
        mov rax, [rsi + 0x38]
        push rax
        popfq

        // Restore callee-saved registers
        mov rbp, [rsi + 0x28]
        mov rbx, [rsi + 0x20]
        mov r12, [rsi + 0x18]
        mov r13, [rsi + 0x10]
        mov r14, [rsi + 0x08]
        mov r15, [rsi + 0x00]

        // =====================================================================
        // Step 3: Resume execution
        // Pops the return address from the target thread's stack into rip.
        // =====================================================================
        ret
    "#
);

extern "C" {
    /// Perform low-level CPU register context switch between threads.
    ///
    /// # Safety
    /// - `prev` must point to a valid, aligned, mutable `CpuContext` struct.
    /// - `next` must point to a valid, aligned, initialized `CpuContext` struct.
    /// - `next.rsp` must point to a valid stack containing a valid return address.
    pub fn switch_context(prev: *mut CpuContext, next: *const CpuContext);
}
