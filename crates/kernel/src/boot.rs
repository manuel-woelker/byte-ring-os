//! Minimal Multiboot v1 adapter for the initial QEMU PC target.

core::arch::global_asm!(include_str!("boot.S"), options(att_syntax));
