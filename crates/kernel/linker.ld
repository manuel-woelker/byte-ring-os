ENTRY(_start)
SECTIONS
{
    . = 1M;
    __image_start = .;
    .multiboot : { KEEP(*(.multiboot)) }
    .text ALIGN(4K) : { *(.text.boot) *(.text .text.*) }
    .rodata ALIGN(4K) : { *(.rodata .rodata.*) }
    .data ALIGN(4K) : { *(.data .data.*) *(.got .got.*) }
    __load_end = .;
    .bss ALIGN(4K) (NOLOAD) : {
        __bss_start = .;
        *(.bss .bss.*) *(COMMON)
        __bss_end = .;
    }
    /DISCARD/ : { *(.eh_frame*) *(.comment) *(.note*) }
    ASSERT(__bss_end < 0x40000000, "kernel must fit the bootstrap identity mapping")
    ASSERT(SIZEOF(.multiboot) == 32, "unexpected Multiboot header layout")
}
