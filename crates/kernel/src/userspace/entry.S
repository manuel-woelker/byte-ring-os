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
    movq USER_ROOT(%rip), %rax
    movq %rax, %cr3
    leaq USER_KERNEL_STACK_TOP(%rip), %rsp
    pushq $0x2b
    pushq %rsi
    pushq $0x202
    pushq $0x33
    pushq %rdi
    movq %rdx, %rdi
    movq %rcx, %rsi
    movw $0x2b, %ax
    movw %ax, %ds
    movw %ax, %es
    fxrstor64 CLEAN_FX_STATE(%rip)
    xorq %rax, %rax
    xorq %rbx, %rbx
    xorq %rcx, %rcx
    xorq %rdx, %rdx
    xorq %rbp, %rbp
    xorq %r8, %r8
    xorq %r9, %r9
    xorq %r10, %r10
    xorq %r11, %r11
    xorq %r12, %r12
    xorq %r13, %r13
    xorq %r14, %r14
    xorq %r15, %r15
    iretq

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
    // SYSCALL does not switch stacks. No user stack access is permitted here.
    movq %rsp, USER_SAVED_RSP(%rip)
    leaq USER_KERNEL_STACK_TOP(%rip), %rsp
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
    subq $512, %rsp
    fxsave64 (%rsp)
    fxrstor64 KERNEL_FX_STATE(%rip)
    pushq $2
    popfq
    cld
    leaq 512(%rsp), %rdi
    call \dispatch
    fxrstor64 (%rsp)
    addq $512, %rsp
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
.endm
user_syscall_saved_frame:
    save_user user_syscall_dispatch
.global user_timer_entry
user_timer_entry:
    save_user user_timer_dispatch
.global user_spurious_entry
user_spurious_entry:
    save_user user_spurious_dispatch

.section .bss.userspace,"aw",@nobits
.balign 16
.global KERNEL_FX_STATE
KERNEL_FX_STATE: .skip 512
KERNEL_RESUME_RSP: .skip 8
KERNEL_RESUME_CR3: .skip 8
USER_SAVED_RSP: .skip 8
.balign 16
    .skip 65536
.global USER_KERNEL_STACK_TOP
USER_KERNEL_STACK_TOP:
