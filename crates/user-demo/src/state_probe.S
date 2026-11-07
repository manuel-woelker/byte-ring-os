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
    popq %rbx
    popq %rbp
    retq
