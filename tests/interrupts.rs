mod common;

use common::*;
use mos6510::{Cpu, Model};

#[test]
fn an_irq_is_taken_after_the_current_instruction() {
    let (mut cpu, mut ram) = setup(&[0xea, 0xea, 0xea]);
    cpu.set_irq(true);
    ticks(&mut cpu, &mut ram, 2);
    assert!(cpu.interrupt_pending());
    ticks(&mut cpu, &mut ram, 7);
    assert_eq!(cpu.pc, IRQ_HANDLER);
    assert!(cpu.at_instruction_boundary());
    assert_eq!(pushed_return(&ram), 0x0201);
    assert_eq!(pushed_status(&ram) & 0x30, 0x20, "B clear, bit 5 set");
    assert_ne!(cpu.p & 0x04, 0, "I set on entry");
    assert_eq!(cpu.s, 0xfc);
}

#[test]
fn the_instruction_after_cli_runs_before_the_irq() {
    let (mut cpu, mut ram) = setup(&[0x58, 0xea, 0xea]); // CLI, NOP, NOP
    cpu.p |= 0x04;
    cpu.set_irq(true);
    ticks(&mut cpu, &mut ram, 2 + 2 + 7);
    assert_eq!(cpu.pc, IRQ_HANDLER);
    assert_eq!(pushed_return(&ram), 0x0202);
}

#[test]
fn an_irq_already_asserted_gets_through_sei() {
    let (mut cpu, mut ram) = setup(&[0x78, 0xea]); // SEI, NOP
    cpu.set_irq(true);
    ticks(&mut cpu, &mut ram, 2 + 7);
    assert_eq!(cpu.pc, IRQ_HANDLER);
    assert_eq!(pushed_return(&ram), 0x0201);
    assert_ne!(pushed_status(&ram) & 0x04, 0, "SEI had already set I");
}

#[test]
fn plp_clearing_i_delays_the_irq_by_one_instruction() {
    let (mut cpu, mut ram) = setup(&[0x28, 0xea, 0xea]); // PLP, NOP, NOP
    cpu.p |= 0x04;
    cpu.s = 0xfe;
    ram.mem[0x01ff] = 0x20; // the status PLP pulls: I clear
    cpu.set_irq(true);
    ticks(&mut cpu, &mut ram, 4 + 2 + 7);
    assert_eq!(cpu.pc, IRQ_HANDLER);
    assert_eq!(pushed_return(&ram), 0x0202);
}

#[test]
fn rti_reenters_at_once_while_the_irq_is_held() {
    let (mut cpu, mut ram) = setup(&[0xea, 0xea]);
    ram.load(IRQ_HANDLER, &[0x40]); // RTI
    cpu.set_irq(true);
    ticks(&mut cpu, &mut ram, 2 + 7 + 6);
    assert_eq!(cpu.pc, 0x0201);
    assert!(
        cpu.interrupt_pending(),
        "RTI restores I before its last poll"
    );
}

#[test]
fn a_three_cycle_instruction_takes_an_irq_raised_in_its_second_cycle() {
    let (mut cpu, mut ram) = setup(&[0xa5, 0x10, 0xea]); // LDA $10, NOP
    ticks(&mut cpu, &mut ram, 1);
    cpu.set_irq(true);
    ticks(&mut cpu, &mut ram, 2 + 7);
    assert_eq!(cpu.pc, IRQ_HANDLER);
    assert_eq!(pushed_return(&ram), 0x0202);
}

#[test]
fn a_taken_branch_without_page_cross_delays_the_irq() {
    let (mut cpu, mut ram) = setup(&[0xd0, 0x00, 0xea, 0xea]); // BNE +0 (taken), NOP
    ticks(&mut cpu, &mut ram, 1);
    cpu.set_irq(true);
    ticks(&mut cpu, &mut ram, 2 + 2 + 7);
    assert_eq!(cpu.pc, IRQ_HANDLER);
    assert_eq!(pushed_return(&ram), 0x0203);
}

#[test]
fn a_held_nmi_is_taken_once() {
    let (mut cpu, mut ram) = setup(&[0xea; 8]);
    cpu.set_nmi(true);
    ticks(&mut cpu, &mut ram, 2 + 7);
    assert_eq!(cpu.pc, NMI_HANDLER);
    ticks(&mut cpu, &mut ram, 2 * 10);
    assert_eq!(cpu.s, 0xfc, "no second entry");
    assert_eq!(cpu.pc, NMI_HANDLER + 10);
}

#[test]
fn an_nmi_during_brk_hijacks_its_vector() {
    let (mut cpu, mut ram) = setup(&[0x00, 0x00]);
    ticks(&mut cpu, &mut ram, 3); // fetch, padding byte, push PCH
    cpu.set_nmi(true);
    ticks(&mut cpu, &mut ram, 4); // push PCL, push P, vector low, vector high
    assert_eq!(cpu.pc, NMI_HANDLER);
    assert_ne!(pushed_status(&ram) & 0x10, 0, "still pushed as a BRK");
    assert_eq!(pushed_return(&ram), 0x0202);
}

#[test]
fn reset_reads_its_vector_without_writing() {
    let mut ram = Ram::new();
    ram.vector(0xfffc, 0x1234);
    let mut cpu = Cpu::new(Model::Mos6502);
    ticks(&mut cpu, &mut ram, 7);
    assert_eq!(cpu.pc, 0x1234);
    assert!(cpu.at_instruction_boundary());
    assert_eq!(cpu.s, 0xfd);
    assert_ne!(cpu.p & 0x04, 0);
    assert!(ram.writes.is_empty());
}

#[test]
fn jam_halts_until_reset() {
    let (mut cpu, mut ram) = setup(&[0x02]);
    ram.vector(0xfffc, IRQ_HANDLER);
    ticks(&mut cpu, &mut ram, 50);
    assert!(cpu.jammed());
    assert!(!cpu.at_instruction_boundary());
    assert_eq!(cpu.pc, 0x0201);
    cpu.reset();
    ticks(&mut cpu, &mut ram, 7);
    assert!(!cpu.jammed());
    assert_eq!(cpu.pc, IRQ_HANDLER);
}

#[test]
fn jump_abandons_the_instruction_in_progress() {
    let (mut cpu, mut ram) = setup(&[0xad, 0x00, 0x10]); // LDA $1000
    ram.load(0x0300, &[0xa9, 0x42]); // LDA #$42
    ticks(&mut cpu, &mut ram, 2);
    cpu.jump(0x0300);
    assert!(cpu.at_instruction_boundary());
    ticks(&mut cpu, &mut ram, 2);
    assert_eq!(cpu.a, 0x42);
    assert_eq!(cpu.pc, 0x0302);
}

#[test]
fn a_late_nmi_during_brk_is_taken_after_the_handlers_first_instruction() {
    let (mut cpu, mut ram) = setup(&[0x00, 0x00]);
    ticks(&mut cpu, &mut ram, 5); // fetch, padding, push PCH, push PCL, push P
    cpu.set_nmi(true);
    ticks(&mut cpu, &mut ram, 2);
    assert_eq!(cpu.pc, IRQ_HANDLER);
    assert!(!cpu.interrupt_pending());
    ticks(&mut cpu, &mut ram, 2); // the handler's first NOP
    assert_eq!(cpu.pc, IRQ_HANDLER + 1);
    assert!(cpu.interrupt_pending());
    ticks(&mut cpu, &mut ram, 7);
    assert_eq!(cpu.pc, NMI_HANDLER);
}

#[test]
fn an_irq_is_not_taken_while_i_is_set() {
    let (mut cpu, mut ram) = setup(&[0xea; 16]);
    cpu.p |= 0x04;
    cpu.set_irq(true);
    ticks(&mut cpu, &mut ram, 20);
    assert_eq!(cpu.pc, 0x020a);
    assert_eq!(cpu.s, 0xff);
}

#[test]
fn an_irq_raised_in_the_last_cycle_waits_for_the_next_instruction() {
    let (mut cpu, mut ram) = setup(&[0xa5, 0x10, 0xea, 0xea]); // LDA $10, NOP, NOP
    ticks(&mut cpu, &mut ram, 2);
    cpu.set_irq(true);
    ticks(&mut cpu, &mut ram, 1); // LDA's last cycle: too late to be polled
    ticks(&mut cpu, &mut ram, 2); // the NOP runs
    ticks(&mut cpu, &mut ram, 7);
    assert_eq!(cpu.pc, IRQ_HANDLER);
    assert_eq!(pushed_return(&ram), 0x0203);
}

/// BNE at $02F0 taking a page-crossing branch to $0302. The IRQ handler sits
/// at $0500 so it cannot overlap the branch target.
fn crossing_branch() -> (Cpu, Ram) {
    let (mut cpu, mut ram) = setup(&[]);
    ram.load(0x02f0, &[0xd0, 0x10]); // BNE +$10: $02F2 + $10 = $0302
    ram.load(0x0302, &[0xea; 16]);
    ram.load(0x0500, &[0xea; 16]);
    ram.vector(0xfffe, 0x0500);
    cpu.jump(0x02f0);
    (cpu, ram)
}

#[test]
fn a_taken_branch_crossing_a_page_polls_before_the_fix_up() {
    let (mut cpu, mut ram) = crossing_branch();
    ticks(&mut cpu, &mut ram, 2); // fetch, operand
    cpu.set_irq(true);
    ticks(&mut cpu, &mut ram, 2); // the dummy read (polled), the fix-up
    ticks(&mut cpu, &mut ram, 7);
    assert_eq!(cpu.pc, 0x0500);
    assert_eq!(pushed_return(&ram), 0x0302);
}

#[test]
fn an_irq_raised_in_the_fix_up_cycle_of_a_page_crossing_branch_waits() {
    let (mut cpu, mut ram) = crossing_branch();
    ticks(&mut cpu, &mut ram, 3); // fetch, operand, dummy read
    cpu.set_irq(true);
    ticks(&mut cpu, &mut ram, 1); // the fix-up: the last cycle, never polled
    ticks(&mut cpu, &mut ram, 2 + 7); // one NOP, then the entry
    assert_eq!(cpu.pc, 0x0500);
    assert_eq!(pushed_return(&ram), 0x0303);
}
