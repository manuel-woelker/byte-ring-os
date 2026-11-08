// QEMU's Multiboot loader enters in 32-bit protected mode, with paging off.
.section .multiboot,"a"
.balign 4
multiboot_header:
    .long 0x1badb002
    .long 0x00010003
    .long -(0x1badb002 + 0x00010003)
    .long multiboot_header
    .long __image_start
    .long __load_end
    .long __bss_end
    .long _start

.section .text.boot,"ax"
.code32
.global _start
_start:
    cli
    cld
    movl %eax, saved_multiboot_magic
    movl %ebx, saved_multiboot_info
    // Do not depend on inherited descriptor registers after leaving this entry.
    lgdt gdt_pointer
    // Zero all BSS, including the page tables, before using the new stack.
    movl $__bss_start, %edi
    movl $__bss_end, %ecx
    subl %edi, %ecx
    xorl %eax, %eax
    rep stosb
    movl $stack_top, %esp

    movl $pdpt, %eax
    orl $3, %eax
    movl %eax, pml4
    movl $page_directory, %eax
    orl $3, %eax
    movl %eax, pdpt
    // Identity map the first GiB using 512 writable, supervisor 2 MiB pages.
    xorl %ecx, %ecx
1:
    movl %ecx, %eax
    shll $21, %eax
    orl $0x83, %eax
    movl %eax, page_directory(,%ecx,8)
    incl %ecx
    cmpl $512, %ecx
    jne 1b

    // Enable PAE and the SSE state management required by the Rust ABI.
    movl %cr4, %eax
    orl $0x620, %eax
    movl %eax, %cr4
    movl $pml4, %eax
    movl %eax, %cr3
    movl $0xc0000080, %ecx
    rdmsr
    orl $0x100, %eax
    wrmsr
    movl %cr0, %eax
    andl $0xfffffffb, %eax
    orl $0x80000002, %eax
    movl %eax, %cr0
    // A far jump loads the 64-bit code descriptor (selector 8).
    .byte 0xea
    .long long_mode_start
    .word 8

.code64
long_mode_start:
    movw $16, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    xorw %ax, %ax
    movw %ax, %fs
    movw %ax, %gs
    leaq stack_top(%rip), %rsp
    xorq %rbp, %rbp
    movl saved_multiboot_magic(%rip), %edi
    movl saved_multiboot_info(%rip), %esi
    call kernel_main
2:
    cli
    hlt
    jmp 2b

.balign 8
gdt:
    .quad 0
    .quad 0x00af9a000000ffff
    .quad 0x00cf92000000ffff
gdt_end:
gdt_pointer:
    .word gdt_end - gdt - 1
    .long gdt

.section .bss,"aw",@nobits
.balign 4096
pml4:
    .skip 4096
.global pdpt
pdpt:
    .skip 4096
page_directory:
    .skip 4096
.balign 16
stack_bottom:
    .skip 65536
stack_top:

// Loader values must survive BSS clearing and the long-mode transition.
.section .data.boot,"aw"
.balign 4
saved_multiboot_magic: .long 0
saved_multiboot_info: .long 0
