//! Boot-CPU-owned descriptor tables. TSS RSP0 follows the selected user task.

use core::arch::asm;
use core::cell::UnsafeCell;
use core::mem::size_of;
use core::ptr::{addr_of, addr_of_mut};

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct Gate {
    offset_low: u16,
    selector: u16,
    ist: u8,
    attributes: u8,
    offset_middle: u16,
    offset_high: u32,
    reserved: u32,
}

impl Gate {
    const EMPTY: Self = Self {
        offset_low: 0,
        selector: 0,
        ist: 0,
        attributes: 0,
        offset_middle: 0,
        offset_high: 0,
        reserved: 0,
    };

    fn interrupt(address: u64, ist: u8) -> Self {
        Self {
            offset_low: address as u16,
            selector: 8,
            ist,
            attributes: 0x8e,
            offset_middle: (address >> 16) as u16,
            offset_high: (address >> 32) as u32,
            reserved: 0,
        }
    }
}

#[repr(C, packed)]
struct DescriptorPointer {
    limit: u16,
    base: u64,
}

#[repr(C, packed)]
struct TaskStateSegment {
    reserved0: u32,
    rsp: [u64; 3],
    reserved1: u64,
    ist: [u64; 7],
    reserved2: u64,
    reserved3: u16,
    io_map_base: u16,
}

#[repr(C, align(16))]
struct Stack([u8; 32 * 1024]);

struct BootCell<T>(UnsafeCell<T>);
// SAFETY: the sole boot CPU installs these objects and updates TSS RSP0 with IF=0.
// Afterwards IDT/GDT stay fixed; the sole CPU updates TSS RSP0 with IF=0. IST stacks are used
// by their assigned exception only. There is no SMP or concurrent mutation.
unsafe impl<T> Sync for BootCell<T> {}

static IDT: BootCell<[Gate; 256]> = BootCell(UnsafeCell::new([Gate::EMPTY; 256]));
const GDT_ENTRIES: usize = if cfg!(feature = "userspace") { 7 } else { 5 };
static GDT: BootCell<[u64; GDT_ENTRIES]> = BootCell(UnsafeCell::new({
    let mut table = [0; GDT_ENTRIES];
    table[1] = 0x00af9a000000ffff;
    table[2] = 0x00cf92000000ffff;
    #[cfg(feature = "userspace")]
    {
        table[5] = 0x00cff2000000ffff;
        table[6] = 0x00affa000000ffff;
    }
    table
}));
static TSS: BootCell<TaskStateSegment> = BootCell(UnsafeCell::new(TaskStateSegment {
    reserved0: 0,
    rsp: [0; 3],
    reserved1: 0,
    ist: [0; 7],
    reserved2: 0,
    reserved3: 0,
    io_map_base: size_of::<TaskStateSegment>() as u16,
}));
static DOUBLE_FAULT_STACK: BootCell<Stack> = BootCell(UnsafeCell::new(Stack([0; 32 * 1024])));
static NMI_STACK: BootCell<Stack> = BootCell(UnsafeCell::new(Stack([0; 32 * 1024])));
static MACHINE_CHECK_STACK: BootCell<Stack> = BootCell(UnsafeCell::new(Stack([0; 32 * 1024])));

unsafe extern "C" {
    static exception_stubs: [u64; 32];
    static unexpected_interrupt_stub: u64;
    #[cfg(feature = "userspace")]
    fn user_timer_entry();
    #[cfg(feature = "userspace")]
    fn user_spurious_entry();
    #[cfg(feature = "network")]
    static irq_stubs: [u64; 16];
    #[cfg(feature = "network")]
    static apic_timer_stub: u64;
    #[cfg(feature = "network")]
    static apic_spurious_stub: u64;
}

const _: () = assert!(size_of::<Gate>() == 16);
const _: () = assert!(size_of::<TaskStateSegment>() == 104);

pub(super) unsafe fn install() {
    // SAFETY: the caller guarantees sole boot CPU ownership with interrupts
    // disabled. All objects have static lifetime and mapped kernel addresses.
    unsafe {
        let tss = TSS.0.get();
        #[cfg(feature = "userspace")]
        addr_of_mut!((*tss).rsp).write_unaligned([
            super::super::userspace::kernel_stack_top(),
            0,
            0,
        ]);
        addr_of_mut!((*tss).ist).write_unaligned([
            DOUBLE_FAULT_STACK.0.get() as u64 + size_of::<Stack>() as u64,
            NMI_STACK.0.get() as u64 + size_of::<Stack>() as u64,
            MACHINE_CHECK_STACK.0.get() as u64 + size_of::<Stack>() as u64,
            0,
            0,
            0,
            0,
        ]);
        let base = tss as u64;
        let limit = (size_of::<TaskStateSegment>() - 1) as u64;
        let gdt = &mut *GDT.0.get();
        gdt[3] = limit | ((base & 0xffffff) << 16) | (0x89 << 40) | (((base >> 24) & 0xff) << 56);
        gdt[4] = base >> 32;
        let gdtr = DescriptorPointer {
            limit: (size_of::<[u64; GDT_ENTRIES]>() - 1) as u16,
            base: GDT.0.get() as u64,
        };
        asm!("lgdt [{}]", in(reg) &gdtr, options(readonly, nostack, preserves_flags));
        // Existing code/data selectors retain identical descriptors.
        asm!("ltr ax", in("ax") 24u16, options(nostack, preserves_flags));

        let idt = &mut *IDT.0.get();
        for (vector, gate) in idt.iter_mut().enumerate() {
            let address = if vector < 32 {
                addr_of!(exception_stubs).cast::<u64>().add(vector).read()
            } else {
                addr_of!(unexpected_interrupt_stub).read()
            };
            #[cfg(feature = "network")]
            let address = if (32..48).contains(&vector) {
                addr_of!(irq_stubs).cast::<u64>().add(vector - 32).read()
            } else if vector == byte_ring_drivers::apic::TIMER_VECTOR as usize {
                addr_of!(apic_timer_stub).read()
            } else if vector == byte_ring_drivers::apic::SPURIOUS_VECTOR as usize {
                addr_of!(apic_spurious_stub).read()
            } else {
                address
            };
            #[cfg(feature = "userspace")]
            let address = match vector {
                48 => user_timer_entry as *const () as u64,
                255 => user_spurious_entry as *const () as u64,
                _ => address,
            };
            let ist = match vector {
                8 => 1,
                2 => 2,
                18 => 3,
                _ => 0,
            };
            *gate = Gate::interrupt(address, ist);
        }
        let idtr = DescriptorPointer {
            limit: (size_of::<[Gate; 256]>() - 1) as u16,
            base: IDT.0.get() as u64,
        };
        asm!("lidt [{}]", in(reg) &idtr, options(readonly, nostack, preserves_flags));
    }
}

#[cfg(feature = "qemu-test")]
pub(super) fn on_double_fault_stack(
    frame: &byte_ring_kernel_core::exception::ExceptionFrame,
) -> bool {
    let start = DOUBLE_FAULT_STACK.0.get() as usize;
    (start..start + size_of::<Stack>()).contains(&(frame as *const _ as usize))
}

/// # Safety
/// Sole CPU, IF=0, stack must be pinned supervisor-only memory for the next task.
#[cfg(feature = "userspace")]
pub(super) unsafe fn set_user_stack(top: u64) {
    // SAFETY: caller serializes TSS mutation while no user entry can occur.
    unsafe {
        addr_of_mut!((*TSS.0.get()).rsp)
            .cast::<u64>()
            .write_unaligned(top);
    }
}
