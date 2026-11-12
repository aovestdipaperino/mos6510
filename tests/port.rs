mod common;

use common::*;
use mos6510::{Cpu, Model};

fn cpu_at(model: Model, program: &[u8]) -> (Cpu, Ram) {
    let mut ram = Ram::new();
    ram.load(0x0200, program);
    let mut cpu = Cpu::new(model);
    cpu.jump(0x0200);
    (cpu, ram)
}

#[test]
fn port_registers_read_back_and_writes_reach_ram() {
    // LDA #$2F; STA $00; LDA #$37; STA $01; LDA $01
    let (mut cpu, mut ram) = cpu_at(
        Model::Mos6510,
        &[0xa9, 0x2f, 0x85, 0x00, 0xa9, 0x37, 0x85, 0x01, 0xa5, 0x01],
    );
    ram.mem[1] = 0x99;
    cpu.set_port_input(0x10);
    ticks(&mut cpu, &mut ram, 2 + 3 + 2 + 3 + 3);
    assert_eq!(cpu.a, 0x37);
    assert_eq!(cpu.port_pins(), 0x37);
    assert_eq!(ram.mem[0], 0x2f, "the write also reaches RAM");
    assert_eq!(ram.mem[1], 0x37, "the write also reaches RAM");
}

#[test]
fn reads_of_the_port_ignore_the_ram_underneath() {
    // LDA $01; LDX $00
    let (mut cpu, mut ram) = cpu_at(Model::Mos6510, &[0xa5, 0x01, 0xa6, 0x00]);
    cpu.set_port_registers(0x2f, 0x37);
    cpu.set_port_input(0x10);
    ram.mem[0] = 0x55;
    ram.mem[1] = 0x55;
    ticks(&mut cpu, &mut ram, 3 + 3);
    assert_eq!(cpu.a, 0x37);
    assert_eq!(cpu.x, 0x2f);
}

#[test]
fn input_bits_follow_the_pins() {
    let (mut cpu, mut ram) = cpu_at(Model::Mos6510, &[0xa5, 0x01]); // LDA $01
    cpu.set_port_input(0xa5);
    ticks(&mut cpu, &mut ram, 3);
    assert_eq!(cpu.a, 0xa5);
    assert_eq!(cpu.port_pins(), 0xa5);
}

#[test]
fn a_6502_has_no_port() {
    let (mut cpu, mut ram) = cpu_at(Model::Mos6502, &[0xa5, 0x01]); // LDA $01
    ram.mem[1] = 0x55;
    ticks(&mut cpu, &mut ram, 3);
    assert_eq!(cpu.a, 0x55);
}
