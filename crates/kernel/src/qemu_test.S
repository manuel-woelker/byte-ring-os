.section .text.qemu_faults,"ax"
.code64
.global trigger_divide
trigger_divide:
    xorl %eax, %eax
    xorl %edx, %edx
    xorl %ecx, %ecx
.global divide_instruction
divide_instruction:
    divq %rcx
    ud2

.global trigger_invalid_opcode
.global invalid_opcode_instruction
trigger_invalid_opcode:
invalid_opcode_instruction:
    ud2

.global trigger_general_protection
trigger_general_protection:
    movw $0x28, %ax
.global general_protection_instruction
general_protection_instruction:
    movw %ax, %ds
    ud2

.global trigger_page_fault
trigger_page_fault:
    movabsq $0x40000000, %rax
.global page_fault_instruction
page_fault_instruction:
    movq (%rax), %rax
    ud2

.global trigger_double_fault
trigger_double_fault:
    // Exception delivery on an unmapped stack must use the double-fault IST.
    movabsq $0x40001000, %rsp
    ud2
