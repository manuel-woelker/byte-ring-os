.section .text.user_probe,"ax"
.global user_state_probe
user_state_probe:
    pushq %rbx
    pushq %rbp
    pushq %r12
    pushq %r13
    pushq %r14
    pushq %r15
    subq $24, %rsp
    stmxcsr 0(%rsp)
    movl $0x3f80, 4(%rsp)
    ldmxcsr 4(%rsp)
    movabsq $0x1122334455667788, %rbx
    movq %rbx, %rbp
    movq %rbx, %r12
    movq %rbx, %r13
    movq %rbx, %r14
    movq %rbx, %r15
    movq %rbx, %rsi
    movq %rbx, %r8
    movq %rbx, %r9
    movq %rbx, %r10
    movq %rbx, %xmm0
    movq %rbx, %xmm15
    xorq %rax, %rax
    xorq %rdi, %rdi
    std
    syscall
    testq %rax, %rax
    jne 1f
    cmpq $5, %rdx
    jne 1f
    movabsq $0x1122334455667788, %rdx
    .irp register,rbx,rbp,r12,r13,r14,r15,rsi,r8,r9,r10
        cmpq %rdx, %\register
        jne 1f
    .endr
    testq %rdi, %rdi
    jne 1f
    movq %xmm0, %rax
    cmpq %rdx, %rax
    jne 1f
    movq %xmm15, %rax
    cmpq %rdx, %rax
    jne 1f
    stmxcsr 8(%rsp)
    cmpl $0x3f80, 8(%rsp)
    jne 1f
    pushfq
    popq %rax
    andq $0x600, %rax
    cmpq $0x200, %rax
    jne 1f
    movq $1, %rax
    jmp 2f
1:
    xorq %rax, %rax
2:
    cld
    ldmxcsr 0(%rsp)
    addq $24, %rsp
    popq %r15
    popq %r14
    popq %r13
    popq %r12
    popq %rbp
    popq %rbx
    retq

// RDI=read-only quantum counter, RSI=threshold, RDX=process-specific pattern.
// There is no syscall or yield in this loop: only real timer preemption can
// advance the counter and allow it to return.
.global user_preemption_probe
user_preemption_probe:
    pushq %rbx
    pushq %rbp
    pushq %r12
    pushq %r13
    pushq %r14
    pushq %r15
    subq $40, %rsp
    stmxcsr 0(%rsp)
    movq %rdx, 24(%rsp)
    movw %ds, 16(%rsp)
    movw %es, 18(%rsp)
    movw %fs, 20(%rsp)
    movw %gs, 22(%rsp)
    movl $0x3f80, %eax
    testq $1, %rdx
    jz 3f
    movl $0x5f80, %eax
3:
    movl %eax, 4(%rsp)
    ldmxcsr 4(%rsp)
    movq %rdx, %rbx
    movq %rdx, %rbp
    movq %rdx, %r12
    movq %rdx, %r13
    movq %rdx, %r14
    movq %rdx, %r15
    movq %rdx, %r8
    movq %rdx, %r9
    movq %rdx, %r10
    movq %rdx, %xmm0
    movq %rdx, %xmm15
    movw $0x2b, %ax
    movw %ax, %ds
    movw $0x33, %ax
    movw %ax, %es
    xorw %ax, %ax
    movw %ax, %fs
    movw %ax, %gs
    testq $1, %rdx
    jz 7f
    movw $0x2b, %ax
    movw %ax, %fs
    movw $0x33, %ax
    movw %ax, %gs
7:
    std
4:
    pushfq
    popq %rax
    testq $0x400, %rax
    jz 5f
    movq (%rdi), %rax
    cmpq %rsi, %rax
    jb 4b
    movq 24(%rsp), %rax
    .irp register,rbx,rbp,r12,r13,r14,r15,r8,r9,r10,rdx
        cmpq %rax, %\register
        jne 5f
    .endr
    movq %xmm0, %rdx
    cmpq %rax, %rdx
    jne 5f
    movq %xmm15, %rdx
    cmpq %rax, %rdx
    jne 5f
    stmxcsr 8(%rsp)
    movl 4(%rsp), %eax
    cmpl %eax, 8(%rsp)
    jne 5f
    movw %ds, %ax
    cmpw $0x2b, %ax
    jne 5f
    movw %es, %ax
    cmpw $0x33, %ax
    jne 5f
    movw %fs, %ax
    testq $1, 24(%rsp)
    jz 8f
    cmpw $0x2b, %ax
    jne 5f
    movw %gs, %ax
    cmpw $0x33, %ax
    jne 5f
    jmp 9f
8:
    testw %ax, %ax
    jne 5f
    movw %gs, %ax
    testw %ax, %ax
    jne 5f
9:
    movq $1, %rax
    jmp 6f
5:
    xorq %rax, %rax
6:
    movq %rax, %rdx
    movw 16(%rsp), %ax
    movw %ax, %ds
    movw 18(%rsp), %ax
    movw %ax, %es
    movw 20(%rsp), %ax
    movw %ax, %fs
    movw 22(%rsp), %ax
    movw %ax, %gs
    movq %rdx, %rax
    cld
    ldmxcsr 0(%rsp)
    addq $40, %rsp
    popq %r15
    popq %r14
    popq %r13
    popq %r12
    popq %rbp
    popq %rbx
    retq
