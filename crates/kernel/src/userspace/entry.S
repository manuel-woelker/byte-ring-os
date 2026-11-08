.section .text.userspace,"ax"
.code64
.global enter_user
enter_user:
    pushq %rbp
    pushq %rbx
    pushq %r12
    pushq %r13
    pushq %r14
    pushq %r15
    movq %rsp, KERNEL_RESUME_RSP(%rip)
    movq %cr3, %rax
    movq %rax, KERNEL_RESUME_CR3(%rip)
    fxsave64 KERNEL_FX_STATE(%rip)
    jmp resume_user

// RDI = pinned Saved*, RSI = CR3, RDX = trusted kernel stack top.
// Called only after all Rust process borrows have ended.
.global resume_user
resume_user:
    cli
    movq %rsi, %cr3
    movq %rdi, %rsi
    movq %rdx, %rsp
    subq $688, %rsp
    movq %rsp, %rdi
    movq $86, %rcx
    cld
    rep movsq
    jmp user_restore

.global leave_user
leave_user:
    cli
    movq KERNEL_RESUME_CR3(%rip), %rax
    movq %rax, %cr3
    movw $16, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    xorw %ax, %ax
    movw %ax, %fs
    movw %ax, %gs
    fxrstor64 KERNEL_FX_STATE(%rip)
    movq KERNEL_RESUME_RSP(%rip), %rsp
    popq %r15
    popq %r14
    popq %r13
    popq %r12
    popq %rbx
    popq %rbp
    retq

.global user_syscall_entry
user_syscall_entry:
    // SYSCALL does not switch stacks; never access the untrusted user stack.
    movq %rsp, USER_SAVED_RSP(%rip)
    movq ACTIVE_KERNEL_STACK(%rip), %rsp
    pushq $0x2b
    pushq USER_SAVED_RSP(%rip)
    pushq %r11
    pushq $0x33
    pushq %rcx
    jmp user_syscall_saved_frame

.macro save_user dispatch
    pushq %rax
    pushq %rbx
    pushq %rcx
    pushq %rdx
    pushq %rdi
    pushq %rsi
    pushq %rbp
    pushq %r8
    pushq %r9
    pushq %r10
    pushq %r11
    pushq %r12
    pushq %r13
    pushq %r14
    pushq %r15
    subq $528, %rsp
    movq %rsp, %rdi
    movq $66, %rcx
    xorq %rax, %rax
    cld
    rep stosq
    fxsave64 (%rsp)
    movq $0, 520(%rsp)
    movw %ds, 512(%rsp)
    movw %es, 514(%rsp)
    movw %fs, 516(%rsp)
    movw %gs, 518(%rsp)
    movw $16, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    xorw %ax, %ax
    movw %ax, %fs
    movw %ax, %gs
    fxrstor64 KERNEL_FX_STATE(%rip)
    pushq $2
    popfq
    cld
    movq %rsp, %rdi
    call \dispatch
    jmp user_restore
.endm
user_syscall_saved_frame:
    save_user user_syscall_dispatch
.global user_timer_entry
user_timer_entry:
    save_user user_timer_dispatch
.global user_spurious_entry
user_spurious_entry:
    save_user user_spurious_dispatch

user_restore:
    fxrstor64 (%rsp)
    movw 512(%rsp), %ax
    movw %ax, %ds
    movw 514(%rsp), %ax
    movw %ax, %es
    movw 516(%rsp), %ax
    movw %ax, %fs
    movw 518(%rsp), %ax
    movw %ax, %gs
    addq $528, %rsp
    popq %r15
    popq %r14
    popq %r13
    popq %r12
    popq %r11
    popq %r10
    popq %r9
    popq %r8
    popq %rbp
    popq %rsi
    popq %rdi
    popq %rdx
    popq %rcx
    popq %rbx
    popq %rax
    iretq

.section .bss.userspace,"aw",@nobits
.balign 16
.global KERNEL_FX_STATE
KERNEL_FX_STATE: .skip 512
KERNEL_RESUME_RSP: .skip 8
KERNEL_RESUME_CR3: .skip 8
USER_SAVED_RSP: .skip 8
.global ACTIVE_KERNEL_STACK
ACTIVE_KERNEL_STACK: .skip 8
