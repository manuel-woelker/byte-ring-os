//! Exact assembly save layout; kernel-only process copies never expose pointers.
use byte_ring_kernel_core::user_abi::*;

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct Context {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    pub rip: u64,
    pub cs: u64,
    pub flags: u64,
    pub rsp: u64,
    pub ss: u64,
}
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub(super) struct Saved {
    pub fx: [u8; 512],
    pub ds: u16,
    pub es: u16,
    pub fs: u16,
    pub gs: u16,
    reserved: u64,
    pub context: Context,
}
impl Saved {
    pub fn new(entry: u64, argument: u64) -> Self {
        let mut fx = [0; 512];
        fx[0] = 0x7f;
        fx[1] = 3;
        fx[24] = 0x80;
        fx[25] = 0x1f;
        Self {
            fx,
            ds: USER_SS as u16,
            es: USER_SS as u16,
            fs: 0,
            gs: 0,
            reserved: 0,
            context: Context {
                r15: 0,
                r14: 0,
                r13: 0,
                r12: 0,
                r11: 0,
                r10: 0,
                r9: 0,
                r8: 0,
                rbp: 0,
                rsi: argument,
                rdi: BOOT,
                rdx: 0,
                rcx: 0,
                rbx: 0,
                rax: 0,
                rip: entry,
                cs: USER_CS,
                flags: 0x202,
                rsp: STACK_END - 8,
                ss: USER_SS,
            },
        }
    }
    pub fn safe_segments(&self) -> bool {
        [self.ds, self.es, self.fs, self.gs]
            .into_iter()
            .all(|selector| matches!(selector, 0 | 0x2b | 0x33))
    }
}
const _: () = assert!(core::mem::size_of::<Saved>() == 688);
const _: () = assert!(core::mem::offset_of!(Saved, context) == 528);
