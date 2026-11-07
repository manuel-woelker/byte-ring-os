// These handlers never resume execution, so no caller registers need saving.
// Normalize error-code and non-error-code vectors to the same C frame.
.section .text.exceptions,"ax"
.code64
.macro exception number error=0
.global exception_\number
exception_\number:
.if !\error
    pushq $0
.endif
    pushq $\number
    jmp exception_common
.endm
exception 0
exception 1
exception 2
exception 3
exception 4
exception 5
exception 6
exception 7
exception 8, 1
exception 9
exception 10, 1
exception 11, 1
exception 12, 1
exception 13, 1
exception 14, 1
exception 15
exception 16
exception 17, 1
exception 18
exception 19
exception 20
exception 21, 1
exception 22
exception 23
exception 24
exception 25
exception 26
exception 27
exception 28
exception 29, 1
exception 30, 1
exception 31
exception 255

exception_common:
    cli
    cld
    movq %cr2, %rax
    pushq %rax
    movq %rsp, %rdi
    andq $-16, %rsp
    call exception_dispatch
    ud2

.section .rodata.exception_stubs,"a"
.balign 8
.global exception_stubs
exception_stubs:
.irp number,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    .quad exception_\number
.endr
.global unexpected_interrupt_stub
unexpected_interrupt_stub:
    .quad exception_255
