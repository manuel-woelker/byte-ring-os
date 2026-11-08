//! Actual ring-3 Rust code; only bootstrap and fixed shared queue mappings exist.
use byte_ring_kernel_core::{
    bytecode::builder::ProgramBuilder,
    queue::{Status, Stop},
    user_abi::*,
};
use core::{
    arch::asm,
    sync::atomic::{AtomicU64, Ordering},
};

core::arch::global_asm!(include_str!("state_probe.S"), options(att_syntax));
unsafe extern "C" {
    fn user_state_probe() -> u64;
    fn user_preemption_probe(counter: u64, threshold: u64, pattern: u64) -> u64;
}
static mut FRESH_BSS: [u8; 32] = [0; 32];

fn call(operation: u64, argument: u64) -> (u64, u64) {
    let result: u64;
    let reason: u64;
    // SAFETY: the advertised native entry saves all other registers and SIMD.
    // This is deliberately not nomem: shared publication must precede entry.
    unsafe {
        asm!("syscall", inlateout("rax") operation => result, in("rdi") argument, lateout("rdx") reason, lateout("rcx") _, lateout("r11") _);
    }
    (result, reason)
}
pub fn exit(status: u64) -> ! {
    let _ = call(1, status);
    loop {
        core::hint::spin_loop();
    }
}
fn counter(address: u64, value: Option<u64>) -> u64 {
    // SAFETY: bootstrap fixes and pins aligned atomic queue counters. The page
    // rights distinguish producer/consumer ownership; these calls use our side.
    let counter = unsafe { &*(address as *const AtomicU64) };
    if let Some(value) = value {
        counter.store(value, Ordering::Release);
    }
    counter.load(Ordering::Acquire)
}
fn submit(sequence: u64, handle: Option<u64>, malformed: bool) {
    submit_bytes(
        sequence,
        handle,
        malformed,
        b"Byte Ring OS: hello from ring 3 via SQ/CQ\r\n",
    );
}
fn submit_bytes(sequence: u64, handle: Option<u64>, malformed: bool, payload: &[u8]) {
    let mut bytes = [0; MAX_PROGRAM];
    let mut program = ProgramBuilder::new(&mut bytes);
    if let Some(handle) = handle {
        program.write(handle, payload).unwrap();
    }
    program.return_imm(123).unwrap();
    let mut entry = [0; SUBMISSION_SIZE];
    assert!(submission(
        77 + sequence,
        program.finish().unwrap(),
        &mut entry
    ));
    if malformed {
        entry[16] = 0xff;
    }
    let address = SQ + ENTRIES as u64 + (sequence & 3) * SUBMISSION_SIZE as u64;
    for (offset, byte) in entry.iter().enumerate() {
        // SAFETY: this unpublished SQ slot is owned by this sole producer.
        unsafe {
            (address as *mut u8).add(offset).write_volatile(*byte);
        }
    }
    counter(SQ, Some(sequence.wrapping_add(1)));
}
fn completion(sequence: u64, expected: Status) {
    assert_eq!(counter(CQ, None), sequence + 1);
    let mut bytes = [0; COMPLETION_SIZE];
    let address = CQ + ENTRIES as u64 + (sequence & 3) * COMPLETION_SIZE as u64;
    for (offset, byte) in bytes.iter_mut().enumerate() {
        // SAFETY: the CQ producer was acquire-loaded before reading its slot.
        *byte = unsafe { (address as *const u8).add(offset).read_volatile() };
    }
    assert_eq!(read_u64(&bytes, 0), sequence);
    assert_eq!(read_u64(&bytes, 8), 77 + sequence);
    assert_eq!(read_u32(&bytes, 16), expected as u32);
    if expected == Status::Success {
        assert_eq!(read_u64(&bytes, 24), 123);
    }
    counter(CQ_CONSUMER, Some(sequence + 1));
}

pub fn run(boot: u64, argument: u64) -> ! {
    let cs: u16;
    // SAFETY: reading CS does not change CPU state.
    unsafe {
        asm!("mov {0:x}, cs", out(reg) cs, options(nomem, nostack, preserves_flags));
    }
    assert_eq!(cs & 3, 3);
    assert_eq!(boot, BOOT);
    let mut descriptor = [0; 128];
    for (offset, byte) in descriptor.iter_mut().enumerate() {
        // SAFETY: RDI is the immutable, mapped bootstrap descriptor.
        *byte = unsafe { (boot as *const u8).add(offset).read_volatile() };
    }
    assert_eq!(descriptor, bootstrap());
    for offset in 0..32 {
        // SAFETY: this ELF-owned BSS array must be private and reset per instance.
        let pointer = core::ptr::addr_of_mut!(FRESH_BSS).cast::<u8>();
        unsafe {
            assert_eq!(pointer.add(offset).read_volatile(), 0);
            pointer.add(offset).write_volatile(0xa5);
        }
    }
    match argument {
        0 => {
            assert_eq!(call(99, 4), (0, Stop::BadArgument as u64));
            assert_eq!(call(0, 0), (0, Stop::BadArgument as u64));
            submit(0, Some(0), false);
            assert_eq!(call(0, 4), (1, Stop::Idle as u64));
            completion(0, Status::Success);
            submit(1, Some(1), false);
            assert_eq!(call(0, 4), (1, Stop::Idle as u64));
            completion(1, Status::Denied);
            submit(2, Some(0), true);
            assert_eq!(call(0, 4), (1, Stop::Idle as u64));
            completion(2, Status::InvalidProgram);
            for sequence in 3..12 {
                submit(sequence, None, false);
                assert_eq!(call(0, 1), (1, Stop::Idle as u64));
                completion(sequence, Status::Success);
            }
            exit(0)
        }
        1 => unsafe {
            (0x100000 as *const u64).read_volatile();
        }, // Supervisor read.
        2 => unsafe {
            (BASE as *mut u8).write_volatile(0);
        }, // RX write.
        3 => unsafe {
            (CQ as *mut u64).write_volatile(999);
        }, // Kernel-owned page.
        4 => unsafe {
            ((STACK - 8) as *const u64).read_volatile();
        },
        5 => unsafe {
            asm!("out dx, al", in("dx") 0x3f8u16, in("al") b'X');
        },
        6 => loop {
            core::hint::spin_loop();
        },
        7 => {
            counter(SQ, Some(u64::MAX));
            assert_eq!(call(0, 4), (0, Stop::QueueFault as u64));
            counter(SQ, Some(0));
            assert_eq!(call(0, 4), (0, Stop::QueueFault as u64));
            exit(0);
        }
        8 => {
            counter(CQ_CONSUMER, Some(1));
            assert_eq!(call(0, 4), (0, Stop::QueueFault as u64));
            exit(0);
        }
        9 => unsafe {
            asm!(
                "mov rsp, 0",
                "xor eax, eax",
                "mov edi, 1",
                "syscall",
                "ud2",
                options(noreturn)
            );
        },
        10 => unsafe {
            (BOOT as *mut u64).write_volatile(0);
        },
        11 => unsafe {
            asm!("jmp rax", in("rax") STACK, options(noreturn));
        },
        12 => {
            for sequence in 0..4 {
                submit(sequence, None, false);
            }
            assert_eq!(call(0, 99), (4, Stop::Idle as u64));
            submit(4, None, false);
            assert_eq!(call(0, 4), (0, Stop::CompletionFull as u64));
            assert_eq!(counter(SQ_HEAD, None), 4);
            counter(CQ_CONSUMER, Some(4));
            assert_eq!(call(0, 4), (1, Stop::Idle as u64));
            exit(0);
        }
        13 => {
            assert_eq!(unsafe { user_state_probe() }, 1);
            exit(0);
        }
        14 => unsafe {
            (0xfee00000 as *mut u32).write_volatile(0);
        },
        15 => unsafe {
            (SQ_HEAD as *mut u64).write_volatile(999);
        },
        16 => unsafe {
            asm!(
                "mov rsp, 0x800000000000",
                "xor eax, eax",
                "mov edi, 1",
                "syscall",
                "ud2",
                options(noreturn)
            );
        },
        17 | 18 => worker(argument),
        _ => exit(98),
    }
    exit(97)
}

fn worker(argument: u64) -> ! {
    let pointer = core::ptr::addr_of_mut!(FRESH_BSS).cast::<u8>();
    for index in 0..32 {
        // SAFETY: our ELF-owned writable BSS; each process must see its own copy.
        unsafe {
            pointer.add(index).write_volatile(argument as u8);
        }
    }
    let messages: [&[u8]; 3] = if argument == 17 {
        [
            b"task A quantum 1\r\n",
            b"task A quantum 2\r\n",
            b"task A quantum 3\r\n",
        ]
    } else {
        [
            b"task B quantum 1\r\n",
            b"task B quantum 2\r\n",
            b"task B quantum 3\r\n",
        ]
    };
    for round in 1..=3 {
        // SAFETY: the read-only diagnostic scalar is pinned and aligned. The
        // probe preserves the Rust C ABI and verifies timer-resumed CPU state.
        assert_eq!(
            unsafe { user_preemption_probe(QUANTA, round, 0x1122334400000000 | argument) },
            1
        );
        for index in 0..32 {
            assert_eq!(
                unsafe { pointer.add(index).read_volatile() },
                argument as u8
            );
        }
        let sequence = round - 1;
        submit_bytes(sequence, Some(0), false, messages[sequence as usize]);
        assert_eq!(call(0, 1), (1, Stop::Idle as u64));
        completion(sequence, Status::Success);
    }
    exit(0)
}
