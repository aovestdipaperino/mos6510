//! The 6510's on-chip I/O port: a data direction register at `$00` and a
//! data register at `$01`. A set direction bit makes the pin an output.

#[derive(Clone, Copy, Debug)]
pub(crate) struct Port {
    pub(crate) ddr: u8,
    pub(crate) data: u8,
    /// What external circuitry pulls each pin to.
    pub(crate) pins: u8,
}

impl Port {
    /// Power-on: every pin an input, the data register clear, pins pulled high.
    pub(crate) const fn new() -> Self {
        Self {
            ddr: 0,
            data: 0,
            pins: 0xff,
        }
    }

    /// The level on each pin: outputs from the data register, inputs from
    /// outside.
    pub(crate) const fn levels(&self) -> u8 {
        (self.data & self.ddr) | (self.pins & !self.ddr)
    }

    pub(crate) const fn read(&self, addr: u16) -> u8 {
        if addr == 0 { self.ddr } else { self.levels() }
    }

    pub(crate) fn write(&mut self, addr: u16, value: u8) {
        if addr == 0 {
            self.ddr = value;
        } else {
            self.data = value;
        }
    }
}
