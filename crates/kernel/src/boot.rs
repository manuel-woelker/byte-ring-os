//! Minimal Multiboot v1 adapter for the initial QEMU PC target.

core::arch::global_asm!(include_str!("boot.S"), options(att_syntax));

/// Copy bounded loader-owned RAM before exposing any selection to userspace.
/// The supported QEMU image has 64 MiB RAM and a Multiboot-v1 command line.
#[cfg(feature = "userspace")]
pub fn scenario(magic: u32, info: u32) -> Result<Option<u64>, &'static str> {
    use byte_ring_kernel_core::boot_options::{self, MAX_COMMAND_LINE};
    let valid = |address: u32, length: usize| {
        address >= 0x1000
            && (address as usize)
                .checked_add(length)
                .is_some_and(|end| end <= 64 * 1024 * 1024)
    };
    if magic != 0x2badb002 || !info.is_multiple_of(4) || !valid(info, 20) {
        return Err("invalid Multiboot information");
    }
    // SAFETY: trusted loader contract supplies readable RAM; range/alignment
    // checks constrain reads to the supported mapped QEMU RAM, before ring 3.
    let flags = unsafe { (info as *const u32).read_volatile() };
    if flags & 4 == 0 {
        return Ok(None);
    }
    let command = unsafe { (info as *const u32).add(4).read_volatile() };
    let mut bytes = [0; MAX_COMMAND_LINE];
    for (index, byte) in bytes.iter_mut().enumerate() {
        if !valid(command, index + 1) {
            return Err("invalid Multiboot command pointer");
        }
        // SAFETY: same loader-owned RAM contract; no borrowed user storage.
        *byte = unsafe { (command as *const u8).add(index).read_volatile() };
        if *byte == 0 {
            return boot_options::scenario(&bytes[..index])
                .map_err(|_| "invalid user-scenario option");
        }
    }
    Err("Multiboot command line is too long")
}
