//! Fixed, pinned user backing and private page tables. No user references escape.
use byte_ring_kernel_core::{
    elf::{Image, Permission},
    queue,
    scheduler::CAPACITY as PROCESSES,
    user_abi::*,
};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicU64, Ordering},
};

const BACKING_PAGES: usize = IMAGE_PAGES + STACK_PAGES + 5;
#[repr(C, align(4096))]
struct Pages([[u8; PAGE]; BACKING_PAGES]);
#[repr(C, align(4096))]
struct Tables {
    root: [u64; 512],
    pdpt: [u64; 512],
    directory: [u64; 512],
    leaves: [u64; 512],
}
struct Cell<T>(UnsafeCell<T>);
// SAFETY: only the boot CPU with IF=0 loads/reinitializes these objects; no
// references are held while user code executes. User sharing uses raw/atomic I/O.
unsafe impl<T> Sync for Cell<T> {}
static PAGES: Cell<[Pages; PROCESSES]> = Cell(UnsafeCell::new(
    [const { Pages([[0; PAGE]; BACKING_PAGES]) }; PROCESSES],
));
static TABLES: Cell<[Tables; PROCESSES]> = Cell(UnsafeCell::new(
    [const {
        Tables {
            root: [0; 512],
            pdpt: [0; 512],
            directory: [0; 512],
            leaves: [0; 512],
        }
    }; PROCESSES],
));
#[repr(C, align(16))]
struct Stacks([[u8; 65536]; PROCESSES]);
static STACKS: Cell<Stacks> = Cell(UnsafeCell::new(Stacks([[0; 65536]; PROCESSES])));
pub fn kernel_stack_top(process: usize) -> u64 {
    assert!(process < PROCESSES);
    STACKS
        .0
        .get()
        .cast::<u8>()
        .wrapping_add((process + 1) * 65536) as u64
}
unsafe extern "C" {
    static pdpt: [u64; 512];
}

fn page(process: usize, index: usize) -> *mut u8 {
    assert!(process < PROCESSES && index < BACKING_PAGES);
    PAGES
        .0
        .get()
        .cast::<u8>()
        .wrapping_add((process * BACKING_PAGES + index) * PAGE)
}
fn map(process: usize, tables: &mut Tables, address: u64, index: usize, permission: Permission) {
    let flags =
        5 | if permission == Permission::Write {
            2
        } else {
            0
        } | if permission == Permission::Execute {
            0
        } else {
            1 << 63
        };
    tables.leaves[((address - BASE) / PAGE as u64) as usize] = page(process, index) as u64 | flags;
}
/// Prepare only when no process is running. Backing never moves; no external
/// pointers select mappings. Writable aliases are supervisor-only in the kernel.
pub fn prepare(process: usize, image: &Image<'_>) -> u64 {
    // SAFETY: caller is the sole inactive-process boot owner, IF=0. This scope
    // ends all mutable references before the CPU enters ring 3.
    unsafe {
        core::ptr::write_bytes(page(process, 0), 0, BACKING_PAGES * PAGE);
        image.load(&mut *page(process, 0).cast::<[u8; IMAGE_PAGES * PAGE]>());
        page(process, IMAGE_PAGES + STACK_PAGES)
            .copy_from_nonoverlapping(bootstrap().as_ptr(), 128);
        let tables = &mut *TABLES.0.get().cast::<Tables>().add(process);
        tables.root.fill(0);
        tables.pdpt.fill(0);
        tables.directory.fill(0);
        tables.leaves.fill(0);
        for index in 0..512 {
            tables.pdpt[index] = core::ptr::addr_of!(pdpt).cast::<u64>().add(index).read();
        }
        tables.root[0] = core::ptr::addr_of!(tables.pdpt) as u64 | 7;
        tables.pdpt[1] = core::ptr::addr_of!(tables.directory) as u64 | 7;
        tables.directory[0] = core::ptr::addr_of!(tables.leaves) as u64 | 7;
        for index in 0..IMAGE_PAGES {
            let permission = image.permission(index);
            if permission != Permission::Absent {
                map(
                    process,
                    tables,
                    BASE + (index * PAGE) as u64,
                    index,
                    permission,
                );
            }
        }
        for index in 0..STACK_PAGES {
            map(
                process,
                tables,
                STACK + (index * PAGE) as u64,
                IMAGE_PAGES + index,
                Permission::Write,
            );
        }
        for (index, (address, permission)) in [
            (BOOT, Permission::Read),
            (SQ, Permission::Write),
            (SQ_HEAD, Permission::Read),
            (CQ, Permission::Read),
            (CQ_CONSUMER, Permission::Write),
        ]
        .into_iter()
        .enumerate()
        {
            map(
                process,
                tables,
                address,
                IMAGE_PAGES + STACK_PAGES + index,
                permission,
            );
        }
        core::ptr::addr_of!(tables.root) as u64
    }
}

/// User memory operations are restricted to these known, permanently pinned
/// pages. No Rust slice/reference is made to application-mutated byte storage.
pub struct Shared {
    process: usize,
}
impl Shared {
    pub fn new(process: usize) -> Self {
        assert!(process < PROCESSES);
        Self { process }
    }
}
fn shared_page(process: usize, index: usize) -> *mut u8 {
    page(process, IMAGE_PAGES + STACK_PAGES + index)
}
fn load(process: usize, index: usize) -> u64 {
    // SAFETY: initialized aligned atomic counters in the selected pinned pool.
    unsafe { (&*shared_page(process, index).cast::<AtomicU64>()).load(Ordering::Acquire) }
}
fn store(process: usize, index: usize, value: u64) {
    // SAFETY: only the selected process's foreground owner publishes counters.
    unsafe {
        (&*shared_page(process, index).cast::<AtomicU64>()).store(value, Ordering::Release);
    }
}
pub fn on_kernel_stack(process: usize, pointer: *const u8) -> bool {
    let top = kernel_stack_top(process);
    (top - 65536..top).contains(&(pointer as u64))
}
/// Read-only diagnostic field, separate from the authoritative SQ head.
pub fn publish_quanta(process: usize, quanta: u64) {
    // SAFETY: aligned kernel-owned scalar, no user write mapping.
    unsafe {
        (&*shared_page(process, 2).add(8).cast::<AtomicU64>()).store(quanta, Ordering::Release);
    }
}
impl queue::Memory for Shared {
    fn submission_tail(&mut self) -> u64 {
        load(self.process, 1)
    }
    fn completion_tail(&mut self) -> u64 {
        load(self.process, 4)
    }
    fn copy_submission(&mut self, slot: usize, output: &mut [u8; SUBMISSION_SIZE]) {
        assert!(slot < CAPACITY as usize);
        for (offset, byte) in output.iter_mut().enumerate() {
            // SAFETY: private process index and bounded slot select pinned RAM.
            *byte = unsafe {
                shared_page(self.process, 1)
                    .add(ENTRIES + slot * SUBMISSION_SIZE + offset)
                    .read_volatile()
            };
        }
    }
    fn publish_submission_head(&mut self, head: u64) {
        store(self.process, 2, head);
    }
    fn write_completion(&mut self, slot: usize, bytes: &[u8; COMPLETION_SIZE]) {
        assert!(slot < CAPACITY as usize);
        for (offset, byte) in bytes.iter().enumerate() {
            // SAFETY: these bounded slots belong only to the selected process.
            unsafe {
                shared_page(self.process, 3)
                    .add(ENTRIES + slot * COMPLETION_SIZE + offset)
                    .write_volatile(*byte);
            }
        }
    }
    fn publish_completion_head(&mut self, head: u64) {
        store(self.process, 3, head);
    }
}
