# mos6510

A cycle-exact MOS 6510 / 6502 CPU core in Rust.

- `Cpu::tick` runs one Φ2 cycle and performs exactly one bus access, dummy
  reads and writes included.
- All 256 opcodes, including every undocumented one. The unstable `ANE`/`LXA`
  use the magic constant `UNSTABLE_MAGIC` (`$EE`).
- Interrupts are sampled where the NMOS part samples them: IRQ is
  level-triggered, NMI edge-triggered, an NMI can hijack `BRK`/IRQ.
- The 6510's I/O port at `$00`/`$01` (`Model::Mos6510`), or none
  (`Model::Mos6502`).
- No dependencies, no `unsafe`.

```rust
use mos6510::{Bus, Cpu, Model};

struct Ram(Vec<u8>);
impl Bus for Ram {
    fn read(&mut self, addr: u16) -> u8 { self.0[usize::from(addr)] }
    fn write(&mut self, addr: u16, value: u8) { self.0[usize::from(addr)] = value; }
}

let mut ram = Ram(vec![0; 0x1_0000]);
let mut cpu = Cpu::new(Model::Mos6510); // runs the reset sequence first
for _ in 0..1000 {
    cpu.tick(&mut ram);
}
```

## Verification

The test data is not in the repository. Fetch it, then run:

```sh
sh scripts/fetch-harte.sh   # Tom Harte's SingleStepTests (MIT), ~1.3 GB
sh scripts/fetch-klaus.sh   # Klaus Dormann's functional test (GPL-3.0, not vendored)
HARTE_DIR=target/test-data/harte KLAUS_DIR=target/test-data/klaus cargo test --release
```

Without the data those two tests print that they were skipped and pass.

## Licence

MIT OR Apache-2.0. Behaviour is derived from documentation and test data;
no GPL emulator source was consulted.
