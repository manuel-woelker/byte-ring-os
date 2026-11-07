//! Boot-owned, uncached identity mappings for validated hardware windows below 4 GiB.

use byte_ring_drivers::pci::Window;
use byte_ring_hal::memory::Memory;
use core::{arch::asm, cell::UnsafeCell, ptr::addr_of_mut};

#[repr(C, align(4096))]
struct Tables([[u64; 512]; 3]);
struct BootTables(UnsafeCell<Tables>);
// SAFETY: one boot CPU initializes mappings with interrupts disabled.
unsafe impl Sync for BootTables {}
static TABLES: BootTables = BootTables(UnsafeCell::new(Tables([[0; 512]; 3])));
unsafe extern "C" {
    static mut pdpt: [u64; 512];
}

/// # Safety
/// Called only during exclusive boot setup, for validated PCI BAR or local APIC windows.
/// No other code may own or access these device register ranges.
pub unsafe fn map(window: Window) -> Result<Memory<'static>, &'static str> {
    let end = window
        .address
        .checked_add(window.length as u64)
        .ok_or("MMIO overflow")?;
    if window.length == 0 || window.address < 0x40000000 || end > 0x100000000 {
        return Err("unsupported MMIO window");
    }
    let mut address = window.address & !0x1fffff;
    // SAFETY: static page tables are exclusively boot-owned, mapped in RAM,
    // and only previously unmapped, non-RAM MMIO pages are added here.
    unsafe {
        let tables = addr_of_mut!((*TABLES.0.get()).0);
        while address < end {
            let directory = (address >> 30) as usize;
            let index = ((address >> 21) & 511) as usize;
            let table = (*tables).as_mut_ptr().add(directory - 1);
            let entry = addr_of_mut!((*table)[index]);
            let mapping = address | 0x9b; // Present, writable, PWT/PCD uncached, 2 MiB.
            if entry.read() != 0 && entry.read() != mapping {
                return Err("overlapping MMIO mapping");
            }
            entry.write(mapping);
            addr_of_mut!(pdpt)
                .cast::<u64>()
                .add(directory)
                .write(table as u64 | 3);
            asm!("invlpg [{}]",in(reg) address,options(nostack,preserves_flags));
            address += 0x200000;
        }
        Ok(Memory::claim(
            window.address as *mut u8,
            window.length,
            window.address,
        ))
    }
}
