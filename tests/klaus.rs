//! Klaus Dormann's 6502 functional test. Every failure is a branch or jump to
//! itself, and so is success, at `$3469` in the published binary; any other
//! trap address names the failing check in `6502_functional_test.lst`.
//!
//! The binary is GPL-3.0 and not in the repository. Fetch it with
//! `sh scripts/fetch-klaus.sh` and run with `KLAUS_DIR=target/test-data/klaus`.
//! The test is ignored by default and panics if the data is missing; run it
//! with `cargo test --release -- --include-ignored`.

use mos6510::{Bus, Cpu, Model};
use std::path::PathBuf;

const SUCCESS: u16 = 0x3469;

struct Flat(Vec<u8>);

impl Bus for Flat {
    fn read(&mut self, addr: u16) -> u8 {
        self.0[usize::from(addr)]
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.0[usize::from(addr)] = value;
    }
}

#[test]
#[ignore = "needs KLAUS_DIR (see scripts/fetch-klaus.sh)"]
fn klaus_dormann_functional_test() {
    let Some(dir) = std::env::var_os("KLAUS_DIR").map(PathBuf::from) else {
        panic!("KLAUS_DIR is not set (run scripts/fetch-klaus.sh)");
    };
    let path = dir.join("6502_functional_test.bin");
    let Ok(image) = std::fs::read(&path) else {
        panic!("{} not found (run scripts/fetch-klaus.sh)", path.display());
    };
    assert_eq!(image.len(), 0x1_0000, "the image must be a full 64 KiB");

    let mut bus = Flat(image);
    let mut cpu = Cpu::new(Model::Mos6502);
    cpu.jump(0x0400);
    let mut last = None;
    let mut instructions = 0u64;
    loop {
        cpu.tick(&mut bus);
        if cpu.at_instruction_boundary() {
            if last == Some(cpu.pc) {
                break;
            }
            last = Some(cpu.pc);
            instructions += 1;
            assert!(instructions < 100_000_000, "runaway at ${:04X}", cpu.pc);
        }
    }
    assert_eq!(
        cpu.pc, SUCCESS,
        "trapped at ${:04X} after {instructions} instructions; look it up in 6502_functional_test.lst",
        cpu.pc
    );
}
