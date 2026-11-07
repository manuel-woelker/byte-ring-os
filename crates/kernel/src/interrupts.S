// Returning IRQ entries: no GPR/SIMD use and no calls or device accesses.
// IF stays clear on return; foreground code owns masking and PIC EOI.
.section .text.interrupts,"ax"
.code64
.macro irq number
irq_\number:
    lock orq $(1 << \number), IRQ_PENDING(%rip)
    andq $-513, 16(%rsp) // Clear IF in saved RFLAGS, after RIP and CS.
    iretq
.endm
.irp number,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15
    irq \number
.endr
.section .rodata.irq_stubs,"a"
.balign 8
.global irq_stubs
irq_stubs:
.irp number,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15
    .quad irq_\number
.endr

.section .text.interrupts,"ax"
apic_timer_entry:
    lock orq $65536, IRQ_PENDING(%rip)
    andq $-513, 16(%rsp)
    iretq
apic_spurious_entry:
    // A spurious local interrupt sets no in-service bit and needs no EOI.
    andq $-513, 16(%rsp)
    iretq

.section .rodata.apic_stubs,"a"
.balign 8
.global apic_timer_stub
apic_timer_stub:
    .quad apic_timer_entry
.global apic_spurious_stub
apic_spurious_stub:
    .quad apic_spurious_entry
