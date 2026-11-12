//! A cycle-exact MOS 6510 / 6502 CPU core.
//!
//! [`Cpu::tick`] advances exactly one Φ2 cycle and performs exactly one bus
//! access, dummy reads and writes included, so a host can run the processor
//! in lockstep with other chips. All 256 opcodes are implemented, including
//! every undocumented one, and interrupts are sampled where the NMOS part
//! samples them.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod decode;
mod ops;
mod port;

use decode::{Kind, Mode, Op, TABLE};
pub use ops::UNSTABLE_MAGIC;
use ops::{B, C, D, I, N, U, V, Z};
use port::Port;

const STACK: u16 = 0x0100;
const NMI_VECTOR: u16 = 0xfffa;
const RESET_VECTOR: u16 = 0xfffc;
const IRQ_VECTOR: u16 = 0xfffe;

/// Memory and I/O as the processor sees them.
pub trait Bus {
    /// Read a byte. Takes `&mut self` because reads have side effects on real
    /// hardware: a CIA clears its interrupt flags when they are read.
    fn read(&mut self, addr: u16) -> u8;
    /// Write a byte.
    fn write(&mut self, addr: u16, value: u8);
}

/// Which part to emulate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    /// The C64's processor: a 6502 with an I/O port at `$00`/`$01`.
    Mos6510,
    /// A plain NMOS 6502, where `$00`/`$01` are ordinary bus addresses.
    Mos6502,
}

/// Why the BRK-shaped sequence is running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    Brk,
    Interrupt,
    Reset,
}

/// The processor. The registers are public so a host can set up a call.
#[derive(Clone, Debug)]
pub struct Cpu {
    /// Accumulator.
    pub a: u8,
    /// X index register.
    pub x: u8,
    /// Y index register.
    pub y: u8,
    /// Stack pointer, an offset into page 1.
    pub s: u8,
    /// Status, `NV-BDIZC`. Bit 5 stays set; only `PLP` and `RTI` clear B.
    pub p: u8,
    /// Program counter.
    pub pc: u16,
    model: Model,
    port: Port,
    op: Op,
    mode: Mode,
    /// Cycle within the current sequence; 0 means the next tick fetches.
    t: u8,
    addr: u16,
    base: u16,
    ptr: u8,
    data: u8,
    entry: Entry,
    reset_pending: bool,
    jammed: bool,
    irq_line: bool,
    nmi_line: bool,
    nmi_edge: bool,
    /// The result of the last interrupt poll: whether the next boundary
    /// starts an interrupt sequence.
    take_int: bool,
    skip_poll: bool,
}

impl Cpu {
    /// A processor at power-on. Its first seven ticks run the reset sequence,
    /// which loads PC from `$FFFC`.
    pub fn new(model: Model) -> Self {
        let mut cpu = Self {
            a: 0,
            x: 0,
            y: 0,
            s: 0,
            p: U | I,
            pc: 0,
            model,
            port: Port::new(),
            op: Op::Nop,
            mode: Mode::Imp,
            t: 0,
            addr: 0,
            base: 0,
            ptr: 0,
            data: 0,
            entry: Entry::Reset,
            reset_pending: false,
            jammed: false,
            irq_line: false,
            nmi_line: false,
            nmi_edge: false,
            take_int: false,
            skip_poll: false,
        };
        cpu.reset();
        cpu
    }

    /// Run the 7-cycle reset sequence from the next tick: three stack reads
    /// that decrement S, then PC from `$FFFC`, with I set.
    pub fn reset(&mut self) {
        self.reset_pending = true;
        self.jammed = false;
        self.take_int = false;
        self.t = 0;
    }

    /// Start executing at `pc` on the next tick, abandoning any instruction in
    /// progress and any pending reset. For hosts that call routines directly.
    pub fn jump(&mut self, pc: u16) {
        self.pc = pc;
        self.t = 0;
        self.reset_pending = false;
        self.jammed = false;
        self.take_int = false;
    }

    /// Drive the IRQ line. Level-triggered: hold it while the source asserts.
    pub fn set_irq(&mut self, asserted: bool) {
        self.irq_line = asserted;
    }

    /// Drive the NMI line. Edge-triggered: an interrupt is requested when it
    /// goes from released to asserted.
    pub fn set_nmi(&mut self, asserted: bool) {
        if asserted && !self.nmi_line {
            self.nmi_edge = true;
        }
        self.nmi_line = asserted;
    }

    /// The next tick starts a new instruction (or an interrupt sequence).
    pub fn at_instruction_boundary(&self) -> bool {
        self.t == 0 && !self.jammed
    }

    /// At a boundary, the next tick begins an interrupt sequence rather than
    /// an opcode fetch.
    pub fn interrupt_pending(&self) -> bool {
        self.at_instruction_boundary() && !self.reset_pending && self.take_int
    }

    /// A JAM opcode ran. Only [`Cpu::reset`] or [`Cpu::jump`] recovers.
    pub fn jammed(&self) -> bool {
        self.jammed
    }

    /// The I/O port's pin levels (`Model::Mos6510`): outputs as written to
    /// `$01`, inputs as set by [`Cpu::set_port_input`].
    pub fn port_output(&self) -> u8 {
        self.port.levels()
    }

    /// What external circuitry pulls the port pins to. Default: all high.
    pub fn set_port_input(&mut self, pins: u8) {
        self.port.pins = pins;
    }

    /// Load the port's direction (`$00`) and data (`$01`) registers directly,
    /// for a host that starts a program without running the code that would
    /// set them.
    pub fn set_port_registers(&mut self, ddr: u8, data: u8) {
        self.port.ddr = ddr;
        self.port.data = data;
    }

    /// Run one Φ2 cycle: exactly one bus access.
    pub fn tick(&mut self, bus: &mut impl Bus) {
        self.skip_poll = false;
        let done = if self.t == 0 {
            self.begin(bus);
            false
        } else {
            self.step(bus)
        };
        if done {
            self.t = 0;
        } else {
            self.t = self.t.saturating_add(1);
            // Polling at the end of every cycle but the last means the poll
            // that decides is the one in the penultimate cycle.
            if !self.skip_poll {
                self.poll();
            }
        }
    }

    fn poll(&mut self) {
        self.take_int = self.nmi_edge || (self.irq_line && self.p & I == 0);
    }

    fn read(&self, bus: &mut impl Bus, addr: u16) -> u8 {
        let value = bus.read(addr);
        if self.model == Model::Mos6510 && addr < 2 {
            self.port.read(addr)
        } else {
            value
        }
    }

    fn write(&mut self, bus: &mut impl Bus, addr: u16, value: u8) {
        if self.model == Model::Mos6510 && addr < 2 {
            self.port.write(addr, value);
        }
        // The write reaches the bus too, so the RAM under the port sees it.
        bus.write(addr, value);
    }

    fn operand(&mut self, bus: &mut impl Bus) -> u8 {
        let v = self.read(bus, self.pc);
        self.pc = self.pc.wrapping_add(1);
        v
    }

    fn stack_push(&mut self, bus: &mut impl Bus, value: u8) {
        self.write(bus, STACK | u16::from(self.s), value);
        self.s = self.s.wrapping_sub(1);
    }

    /// Cycle 0: an opcode fetch, or the dummy fetch that starts an interrupt
    /// or reset sequence.
    fn begin(&mut self, bus: &mut impl Bus) {
        if self.reset_pending || self.take_int {
            self.read(bus, self.pc);
            self.entry = if self.reset_pending {
                Entry::Reset
            } else {
                Entry::Interrupt
            };
            self.reset_pending = false;
            self.take_int = false;
            self.op = Op::Brk;
            self.mode = Mode::Imp;
            self.skip_poll = true;
            return;
        }
        let opcode = self.operand(bus);
        let (op, mode) = TABLE[usize::from(opcode)];
        self.op = op;
        self.mode = mode;
        self.entry = Entry::Brk;
    }

    /// Cycles 1 and later. Returns true on the instruction's last cycle.
    fn step(&mut self, bus: &mut impl Bus) -> bool {
        match self.op {
            Op::Brk => self.brk(bus),
            Op::Jsr => self.jsr(bus),
            Op::Rts => self.rts(bus),
            Op::Rti => self.rti(bus),
            Op::Jmp => self.jmp(bus),
            Op::JmpInd => self.jmp_ind(bus),
            Op::Pha | Op::Php => self.push_op(bus),
            Op::Pla | Op::Plp => self.pull_op(bus),
            Op::Jam => self.jam(bus),
            Op::Bpl | Op::Bmi | Op::Bvc | Op::Bvs | Op::Bcc | Op::Bcs | Op::Bne | Op::Beq => {
                self.branch(bus)
            }
            _ => match self.mode {
                Mode::Imp => {
                    self.read(bus, self.pc);
                    self.implied();
                    true
                }
                Mode::Acc => {
                    self.read(bus, self.pc);
                    self.a = self.modify(self.a);
                    true
                }
                Mode::Imm => {
                    let v = self.operand(bus);
                    self.exec_read(v);
                    true
                }
                _ => self.memory(bus),
            },
        }
    }

    /// `BRK`, IRQ, NMI and reset share one 7-cycle shape. The vector is chosen
    /// in cycle 4, so an NMI arriving before then hijacks a BRK or an IRQ.
    fn brk(&mut self, bus: &mut impl Bus) -> bool {
        self.skip_poll = true;
        match self.t {
            1 => {
                // The poll after the fetch cycle must not re-enter once BRK ends.
                self.take_int = false;
                self.read(bus, self.pc);
                if self.entry == Entry::Brk {
                    self.pc = self.pc.wrapping_add(1);
                }
                false
            }
            2 => {
                self.brk_push(bus, (self.pc >> 8) as u8);
                false
            }
            3 => {
                self.brk_push(bus, self.pc as u8);
                false
            }
            4 => {
                let b = if self.entry == Entry::Brk { B } else { 0 };
                self.brk_push(bus, (self.p & !B) | U | b);
                self.addr = if self.entry == Entry::Reset {
                    RESET_VECTOR
                } else if self.nmi_edge {
                    self.nmi_edge = false;
                    NMI_VECTOR
                } else {
                    IRQ_VECTOR
                };
                false
            }
            5 => {
                self.data = self.read(bus, self.addr);
                self.p |= I;
                false
            }
            _ => {
                let hi = self.read(bus, self.addr.wrapping_add(1));
                self.pc = u16::from_le_bytes([self.data, hi]);
                true
            }
        }
    }

    /// Reset goes through the motions of the pushes as reads.
    fn brk_push(&mut self, bus: &mut impl Bus, value: u8) {
        if self.entry == Entry::Reset {
            self.read(bus, STACK | u16::from(self.s));
            self.s = self.s.wrapping_sub(1);
        } else {
            self.stack_push(bus, value);
        }
    }

    fn jsr(&mut self, bus: &mut impl Bus) -> bool {
        match self.t {
            1 => {
                self.data = self.operand(bus);
                false
            }
            2 => {
                self.read(bus, STACK | u16::from(self.s));
                false
            }
            3 => {
                self.stack_push(bus, (self.pc >> 8) as u8);
                false
            }
            4 => {
                self.stack_push(bus, self.pc as u8);
                false
            }
            _ => {
                // The high byte is fetched only after the pushes.
                let hi = self.read(bus, self.pc);
                self.pc = u16::from_le_bytes([self.data, hi]);
                true
            }
        }
    }

    fn rts(&mut self, bus: &mut impl Bus) -> bool {
        match self.t {
            1 => {
                self.read(bus, self.pc);
                false
            }
            2 => {
                self.read(bus, STACK | u16::from(self.s));
                self.s = self.s.wrapping_add(1);
                false
            }
            3 => {
                self.data = self.read(bus, STACK | u16::from(self.s));
                self.s = self.s.wrapping_add(1);
                false
            }
            4 => {
                let hi = self.read(bus, STACK | u16::from(self.s));
                self.pc = u16::from_le_bytes([self.data, hi]);
                false
            }
            _ => {
                self.read(bus, self.pc);
                self.pc = self.pc.wrapping_add(1);
                true
            }
        }
    }

    fn rti(&mut self, bus: &mut impl Bus) -> bool {
        match self.t {
            1 => {
                self.read(bus, self.pc);
                false
            }
            2 => {
                self.read(bus, STACK | u16::from(self.s));
                self.s = self.s.wrapping_add(1);
                false
            }
            3 => {
                let v = self.read(bus, STACK | u16::from(self.s));
                self.p = (v & !B) | U;
                self.s = self.s.wrapping_add(1);
                false
            }
            4 => {
                self.data = self.read(bus, STACK | u16::from(self.s));
                self.s = self.s.wrapping_add(1);
                false
            }
            _ => {
                let hi = self.read(bus, STACK | u16::from(self.s));
                self.pc = u16::from_le_bytes([self.data, hi]);
                true
            }
        }
    }

    fn jmp(&mut self, bus: &mut impl Bus) -> bool {
        if self.t == 1 {
            self.data = self.operand(bus);
            return false;
        }
        let hi = self.read(bus, self.pc);
        self.pc = u16::from_le_bytes([self.data, hi]);
        true
    }

    /// `JMP (ind)` with the NMOS page-wrap bug: the high byte comes from the
    /// start of the same page when the pointer ends in `$FF`.
    fn jmp_ind(&mut self, bus: &mut impl Bus) -> bool {
        match self.t {
            1 => {
                self.data = self.operand(bus);
                false
            }
            2 => {
                let hi = self.operand(bus);
                self.base = u16::from_le_bytes([self.data, hi]);
                false
            }
            3 => {
                self.data = self.read(bus, self.base);
                false
            }
            _ => {
                let hi_addr = (self.base & 0xff00) | (self.base.wrapping_add(1) & 0x00ff);
                let hi = self.read(bus, hi_addr);
                self.pc = u16::from_le_bytes([self.data, hi]);
                true
            }
        }
    }

    fn push_op(&mut self, bus: &mut impl Bus) -> bool {
        if self.t == 1 {
            self.read(bus, self.pc);
            return false;
        }
        let v = if self.op == Op::Pha {
            self.a
        } else {
            self.p | B | U
        };
        self.stack_push(bus, v);
        true
    }

    fn pull_op(&mut self, bus: &mut impl Bus) -> bool {
        match self.t {
            1 => {
                self.read(bus, self.pc);
                false
            }
            2 => {
                self.read(bus, STACK | u16::from(self.s));
                self.s = self.s.wrapping_add(1);
                false
            }
            _ => {
                let v = self.read(bus, STACK | u16::from(self.s));
                if self.op == Op::Pla {
                    self.a = v;
                    ops::set_nz(&mut self.p, v);
                } else {
                    self.p = (v & !B) | U;
                }
                true
            }
        }
    }

    /// The processor locks up: after the fetch it reads PC, then `$FFFF`,
    /// `$FFFE`, `$FFFE`, then `$FFFF` forever.
    fn jam(&mut self, bus: &mut impl Bus) -> bool {
        self.skip_poll = true;
        match self.t {
            1 => {
                self.read(bus, self.pc);
                self.jammed = true;
            }
            2 => {
                self.read(bus, 0xffff);
            }
            3 | 4 => {
                self.read(bus, 0xfffe);
            }
            _ => {
                self.read(bus, 0xffff);
            }
        }
        false
    }

    fn taken(&self) -> bool {
        match self.op {
            Op::Bpl => self.p & N == 0,
            Op::Bmi => self.p & N != 0,
            Op::Bvc => self.p & V == 0,
            Op::Bvs => self.p & V != 0,
            Op::Bcc => self.p & C == 0,
            Op::Bcs => self.p & C != 0,
            Op::Bne => self.p & Z == 0,
            _ => self.p & Z != 0,
        }
    }

    /// Branches poll before the operand fetch and, when a page is crossed,
    /// before the high-byte fix. A taken branch that stays on its page does
    /// not poll again, so it delays an interrupt by one instruction.
    fn branch(&mut self, bus: &mut impl Bus) -> bool {
        match self.t {
            1 => {
                self.data = self.operand(bus);
                if !self.taken() {
                    return true;
                }
                self.skip_poll = true;
                false
            }
            2 => {
                self.read(bus, self.pc);
                let target = self.pc.wrapping_add(self.data as i8 as u16);
                self.addr = target;
                self.pc = (self.pc & 0xff00) | (target & 0x00ff);
                self.pc == target
            }
            _ => {
                self.read(bus, self.pc);
                self.pc = self.addr;
                true
            }
        }
    }

    /// Instructions that reach an operand in memory: the address cycles, the
    /// page-cross cycle for indexed modes, then the access.
    fn memory(&mut self, bus: &mut impl Bus) -> bool {
        let (steps, indexed) = match self.mode {
            Mode::Zp => (1, false),
            Mode::Zpx | Mode::Zpy | Mode::Abs => (2, false),
            Mode::Izx => (4, false),
            Mode::Abx | Mode::Aby => (2, true),
            Mode::Izy => (3, true),
            Mode::Imp | Mode::Acc | Mode::Imm => unreachable!("handled in step"),
        };
        let t = self.t;
        if t <= steps {
            self.address(bus, t);
            return false;
        }
        let mut first = steps + 1;
        if indexed {
            if t == first {
                // The read before the high byte is fixed. When no page was
                // crossed it is already the right address, and a read
                // instruction finishes here.
                let unfixed = (self.base & 0xff00) | (self.addr & 0x00ff);
                let v = self.read(bus, unfixed);
                if self.op.kind() == Kind::Read && unfixed == self.addr {
                    self.exec_read(v);
                    return true;
                }
                return false;
            }
            first += 1;
        }
        self.access(bus, t - first)
    }

    fn address(&mut self, bus: &mut impl Bus, t: u8) {
        match (self.mode, t) {
            (Mode::Zp, _) => self.addr = u16::from(self.operand(bus)),
            (Mode::Zpx | Mode::Zpy | Mode::Izx | Mode::Izy, 1) => self.ptr = self.operand(bus),
            (Mode::Zpx, _) => {
                self.read(bus, u16::from(self.ptr));
                self.addr = u16::from(self.ptr.wrapping_add(self.x));
            }
            (Mode::Zpy, _) => {
                self.read(bus, u16::from(self.ptr));
                self.addr = u16::from(self.ptr.wrapping_add(self.y));
            }
            (Mode::Abs | Mode::Abx | Mode::Aby, 1) => self.data = self.operand(bus),
            (Mode::Abs, _) => {
                let hi = self.operand(bus);
                self.addr = u16::from_le_bytes([self.data, hi]);
            }
            (Mode::Abx | Mode::Aby, _) => {
                let hi = self.operand(bus);
                self.base = u16::from_le_bytes([self.data, hi]);
                let index = if self.mode == Mode::Abx {
                    self.x
                } else {
                    self.y
                };
                self.addr = self.base.wrapping_add(u16::from(index));
            }
            (Mode::Izx, 2) => {
                self.read(bus, u16::from(self.ptr));
                self.ptr = self.ptr.wrapping_add(self.x);
            }
            (Mode::Izx, 3) => self.data = self.read(bus, u16::from(self.ptr)),
            (Mode::Izx, _) => {
                let hi = self.read(bus, u16::from(self.ptr.wrapping_add(1)));
                self.addr = u16::from_le_bytes([self.data, hi]);
            }
            (Mode::Izy, 2) => self.data = self.read(bus, u16::from(self.ptr)),
            (Mode::Izy, _) => {
                let hi = self.read(bus, u16::from(self.ptr.wrapping_add(1)));
                self.base = u16::from_le_bytes([self.data, hi]);
                self.addr = self.base.wrapping_add(u16::from(self.y));
            }
            _ => unreachable!("{:?} has no address cycles", self.mode),
        }
    }

    /// The access at the effective address. `k` counts from 0.
    fn access(&mut self, bus: &mut impl Bus, k: u8) -> bool {
        match self.op.kind() {
            Kind::Read => {
                let v = self.read(bus, self.addr);
                self.exec_read(v);
                true
            }
            Kind::Write => {
                let (addr, v) = self.store();
                self.write(bus, addr, v);
                true
            }
            Kind::Rmw => match k {
                0 => {
                    self.data = self.read(bus, self.addr);
                    false
                }
                1 => {
                    // The NMOS part writes the unmodified value back first.
                    self.write(bus, self.addr, self.data);
                    self.data = self.modify(self.data);
                    false
                }
                _ => {
                    self.write(bus, self.addr, self.data);
                    true
                }
            },
            Kind::Other => unreachable!("{:?} does not access memory", self.op),
        }
    }

    fn exec_read(&mut self, v: u8) {
        let p = &mut self.p;
        match self.op {
            Op::Lda => {
                self.a = v;
                ops::set_nz(p, v);
            }
            Op::Ldx => {
                self.x = v;
                ops::set_nz(p, v);
            }
            Op::Ldy => {
                self.y = v;
                ops::set_nz(p, v);
            }
            Op::Lax => {
                self.a = v;
                self.x = v;
                ops::set_nz(p, v);
            }
            Op::Ora => {
                self.a |= v;
                ops::set_nz(p, self.a);
            }
            Op::And => {
                self.a &= v;
                ops::set_nz(p, self.a);
            }
            Op::Eor => {
                self.a ^= v;
                ops::set_nz(p, self.a);
            }
            Op::Adc => self.a = ops::adc(p, self.a, v),
            Op::Sbc => self.a = ops::sbc(p, self.a, v),
            Op::Cmp => ops::compare(p, self.a, v),
            Op::Cpx => ops::compare(p, self.x, v),
            Op::Cpy => ops::compare(p, self.y, v),
            Op::Bit => ops::bit(p, self.a, v),
            Op::Anc => self.a = ops::anc(p, self.a, v),
            Op::Alr => self.a = ops::alr(p, self.a, v),
            Op::Arr => self.a = ops::arr(p, self.a, v),
            Op::Sbx => self.x = ops::sbx(p, self.a, self.x, v),
            Op::Ane => self.a = ops::ane(p, self.a, self.x, v),
            Op::Lxa => {
                let r = ops::lxa(p, self.a, v);
                self.a = r;
                self.x = r;
            }
            Op::Las => {
                let r = v & self.s;
                self.a = r;
                self.x = r;
                self.s = r;
                ops::set_nz(p, r);
            }
            Op::Nop => {}
            _ => unreachable!("{:?} does not read", self.op),
        }
    }

    fn modify(&mut self, v: u8) -> u8 {
        let p = &mut self.p;
        match self.op {
            Op::Asl => ops::asl(p, v),
            Op::Lsr => ops::lsr(p, v),
            Op::Rol => ops::rol(p, v),
            Op::Ror => ops::ror(p, v),
            Op::Inc => {
                let r = v.wrapping_add(1);
                ops::set_nz(p, r);
                r
            }
            Op::Dec => {
                let r = v.wrapping_sub(1);
                ops::set_nz(p, r);
                r
            }
            Op::Slo => {
                let r = ops::asl(p, v);
                self.a |= r;
                ops::set_nz(p, self.a);
                r
            }
            Op::Rla => {
                let r = ops::rol(p, v);
                self.a &= r;
                ops::set_nz(p, self.a);
                r
            }
            Op::Sre => {
                let r = ops::lsr(p, v);
                self.a ^= r;
                ops::set_nz(p, self.a);
                r
            }
            Op::Rra => {
                let r = ops::ror(p, v);
                self.a = ops::adc(p, self.a, r);
                r
            }
            Op::Dcp => {
                let r = v.wrapping_sub(1);
                ops::compare(p, self.a, r);
                r
            }
            Op::Isc => {
                let r = v.wrapping_add(1);
                self.a = ops::sbc(p, self.a, r);
                r
            }
            _ => unreachable!("{:?} does not modify", self.op),
        }
    }

    /// The address and value of a store. `SHA SHX SHY TAS` AND the value with
    /// the base address's high byte + 1, and on a page crossing that value
    /// also replaces the high byte of the address.
    fn store(&mut self) -> (u16, u8) {
        match self.op {
            Op::Sta => (self.addr, self.a),
            Op::Stx => (self.addr, self.x),
            Op::Sty => (self.addr, self.y),
            Op::Sax => (self.addr, self.a & self.x),
            Op::Sha => self.unstable_store(self.a & self.x),
            Op::Shx => self.unstable_store(self.x),
            Op::Shy => self.unstable_store(self.y),
            Op::Tas => {
                self.s = self.a & self.x;
                self.unstable_store(self.s)
            }
            _ => unreachable!("{:?} does not store", self.op),
        }
    }

    fn unstable_store(&self, value: u8) -> (u16, u8) {
        let v = value & ((self.base >> 8) as u8).wrapping_add(1);
        if self.addr & 0xff00 == self.base & 0xff00 {
            (self.addr, v)
        } else {
            ((u16::from(v) << 8) | (self.addr & 0x00ff), v)
        }
    }

    fn implied(&mut self) {
        let p = &mut self.p;
        match self.op {
            Op::Tax => {
                self.x = self.a;
                ops::set_nz(p, self.x);
            }
            Op::Tay => {
                self.y = self.a;
                ops::set_nz(p, self.y);
            }
            Op::Txa => {
                self.a = self.x;
                ops::set_nz(p, self.a);
            }
            Op::Tya => {
                self.a = self.y;
                ops::set_nz(p, self.a);
            }
            Op::Tsx => {
                self.x = self.s;
                ops::set_nz(p, self.x);
            }
            Op::Txs => self.s = self.x,
            Op::Inx => {
                self.x = self.x.wrapping_add(1);
                ops::set_nz(p, self.x);
            }
            Op::Iny => {
                self.y = self.y.wrapping_add(1);
                ops::set_nz(p, self.y);
            }
            Op::Dex => {
                self.x = self.x.wrapping_sub(1);
                ops::set_nz(p, self.x);
            }
            Op::Dey => {
                self.y = self.y.wrapping_sub(1);
                ops::set_nz(p, self.y);
            }
            Op::Clc => *p &= !C,
            Op::Sec => *p |= C,
            Op::Cli => *p &= !I,
            Op::Sei => *p |= I,
            Op::Clv => *p &= !V,
            Op::Cld => *p &= !D,
            Op::Sed => *p |= D,
            Op::Nop => {}
            _ => unreachable!("{:?} is not implied", self.op),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Ram(Box<[u8; 0x1_0000]>);

    impl Bus for Ram {
        fn read(&mut self, addr: u16) -> u8 {
            self.0[usize::from(addr)]
        }
        fn write(&mut self, addr: u16, value: u8) {
            self.0[usize::from(addr)] = value;
        }
    }

    /// BRK at $0200, IRQ handler of NOPs at $0400, NMI handler of NOPs at $0300.
    fn brk_setup() -> (Cpu, Ram) {
        let mut ram = Ram(Box::new([0xEA; 0x1_0000]));
        ram.0[0x0200] = 0x00;
        ram.0[0x0201] = 0x00;
        ram.0[0xFFFE] = 0x00;
        ram.0[0xFFFF] = 0x04;
        ram.0[0xFFFA] = 0x00;
        ram.0[0xFFFB] = 0x03;
        let mut cpu = Cpu::new(Model::Mos6502);
        cpu.s = 0xFF;
        cpu.p = 0x20;
        cpu.jump(0x0200);
        (cpu, ram)
    }

    fn tick_n(cpu: &mut Cpu, ram: &mut Ram, n: usize) {
        for _ in 0..n {
            cpu.tick(ram);
        }
    }

    #[test]
    fn an_nmi_on_brks_fetch_hijacks_it_without_a_second_entry() {
        let (mut cpu, mut ram) = brk_setup();
        cpu.set_nmi(true);
        tick_n(&mut cpu, &mut ram, 7);
        assert_eq!(cpu.pc, 0x0300);
        assert!(!cpu.interrupt_pending());
        tick_n(&mut cpu, &mut ram, 2);
        assert_eq!(cpu.pc, 0x0301);
        assert_eq!(cpu.s, 0xFC);
    }

    #[test]
    fn an_irq_on_brks_fetch_is_not_taken_again_after_brk() {
        let (mut cpu, mut ram) = brk_setup();
        cpu.set_irq(true);
        tick_n(&mut cpu, &mut ram, 7);
        assert_eq!(cpu.pc, 0x0400);
        assert!(!cpu.interrupt_pending());
        tick_n(&mut cpu, &mut ram, 2);
        assert_eq!(cpu.pc, 0x0401);
        assert_eq!(cpu.s, 0xFC);
    }
}
