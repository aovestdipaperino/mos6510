#![allow(dead_code)]

use mos6510::{Bus, Cpu, Model};

pub struct Ram {
    pub mem: Vec<u8>,
    pub writes: Vec<(u16, u8)>,
}

impl Ram {
    pub fn new() -> Self {
        Self {
            mem: vec![0; 0x1_0000],
            writes: Vec::new(),
        }
    }

    pub fn load(&mut self, at: u16, bytes: &[u8]) {
        let at = usize::from(at);
        self.mem[at..at + bytes.len()].copy_from_slice(bytes);
    }

    pub fn vector(&mut self, at: u16, target: u16) {
        self.load(at, &target.to_le_bytes());
    }
}

impl Bus for Ram {
    fn read(&mut self, addr: u16) -> u8 {
        self.mem[usize::from(addr)]
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.mem[usize::from(addr)] = value;
        self.writes.push((addr, value));
    }
}

pub const IRQ_HANDLER: u16 = 0x0300;
pub const NMI_HANDLER: u16 = 0x0400;

/// A 6502 about to run `program` at `$0200`, with S = `$FF`, every flag clear
/// (I included), and IRQ/NMI handlers made of NOPs.
pub fn setup(program: &[u8]) -> (Cpu, Ram) {
    let mut ram = Ram::new();
    ram.load(0x0200, program);
    ram.load(IRQ_HANDLER, &[0xea; 16]);
    ram.load(NMI_HANDLER, &[0xea; 16]);
    ram.vector(0xfffe, IRQ_HANDLER);
    ram.vector(0xfffa, NMI_HANDLER);
    ram.vector(0xfffc, 0x0200);
    let mut cpu = Cpu::new(Model::Mos6502);
    cpu.s = 0xff;
    cpu.p = 0x20;
    cpu.jump(0x0200);
    (cpu, ram)
}

pub fn ticks(cpu: &mut Cpu, ram: &mut Ram, n: usize) {
    for _ in 0..n {
        cpu.tick(ram);
    }
}

/// The return address of an interrupt taken with S = `$FF`.
pub fn pushed_return(ram: &Ram) -> u16 {
    u16::from_le_bytes([ram.mem[0x01fe], ram.mem[0x01ff]])
}

/// The status byte of an interrupt taken with S = `$FF`.
pub fn pushed_status(ram: &Ram) -> u8 {
    ram.mem[0x01fd]
}
