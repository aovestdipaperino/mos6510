//! Tom Harte's SingleStepTests for the NMOS 6502 (MIT licence), compared cycle
//! by cycle: every bus access's address, value and direction, then the final
//! registers and memory.
//!
//! The data (~1.3 GB) is not in the repository. Fetch it with
//! `sh scripts/fetch-harte.sh` and run with `HARTE_DIR=target/test-data/harte`.
//! Without it the test is skipped. `HARTE_OPS=69,6b` limits the run.

use mos6510::{Bus, Cpu, Model};
use serde_json::Value;
use std::path::PathBuf;

/// Flat RAM that logs every access as (address, value, is_write).
struct TestBus {
    mem: Vec<u8>,
    log: Vec<(u16, u8, bool)>,
}

impl Bus for TestBus {
    fn read(&mut self, addr: u16) -> u8 {
        let v = self.mem[usize::from(addr)];
        self.log.push((addr, v, false));
        v
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.mem[usize::from(addr)] = value;
        self.log.push((addr, value, true));
    }
}

fn num(v: &Value) -> u64 {
    v.as_u64().expect("a number in the test data")
}

fn byte(v: &Value) -> u8 {
    u8::try_from(num(v)).expect("a byte")
}

fn word(v: &Value) -> u16 {
    u16::try_from(num(v)).expect("a 16-bit value")
}

fn run_case(case: &Value, bus: &mut TestBus) -> Result<(), String> {
    let init = &case["initial"];
    let fin = &case["final"];
    let ram_init = init["ram"].as_array().expect("initial ram");
    let ram_final = fin["ram"].as_array().expect("final ram");
    for e in ram_init {
        bus.mem[usize::from(word(&e[0]))] = byte(&e[1]);
    }
    bus.log.clear();

    let mut cpu = Cpu::new(Model::Mos6502);
    cpu.a = byte(&init["a"]);
    cpu.x = byte(&init["x"]);
    cpu.y = byte(&init["y"]);
    cpu.s = byte(&init["s"]);
    cpu.p = byte(&init["p"]);
    cpu.jump(word(&init["pc"]));

    let cycles = case["cycles"].as_array().expect("cycles");
    for _ in 0..cycles.len() {
        cpu.tick(bus);
    }

    let mut result = Ok(());
    for (i, want) in cycles.iter().enumerate() {
        let want = (
            word(&want[0]),
            byte(&want[1]),
            want[2].as_str() == Some("write"),
        );
        let got = bus.log.get(i).copied();
        if got != Some(want) {
            result = Err(format!(
                "cycle {i}: got {got:?}, want {want:?} (addr, value, write)"
            ));
            break;
        }
    }
    if result.is_ok() && bus.log.len() != cycles.len() {
        result = Err(format!(
            "{} bus accesses, want {}",
            bus.log.len(),
            cycles.len()
        ));
    }
    if result.is_ok() {
        let regs = [
            ("pc", u64::from(cpu.pc), num(&fin["pc"])),
            ("s", u64::from(cpu.s), num(&fin["s"])),
            ("a", u64::from(cpu.a), num(&fin["a"])),
            ("x", u64::from(cpu.x), num(&fin["x"])),
            ("y", u64::from(cpu.y), num(&fin["y"])),
            ("p", u64::from(cpu.p), num(&fin["p"])),
        ];
        for (name, got, want) in regs {
            if got != want {
                result = Err(format!("{name}: got {got:#x}, want {want:#x}"));
                break;
            }
        }
    }
    if result.is_ok() {
        for e in ram_final {
            let (addr, want) = (word(&e[0]), byte(&e[1]));
            let got = bus.mem[usize::from(addr)];
            if got != want {
                result = Err(format!(
                    "ram[{addr:#06x}]: got {got:#04x}, want {want:#04x}"
                ));
                break;
            }
        }
    }

    // Leave the memory all zeros for the next case without clearing 64 KiB.
    for e in ram_init.iter().chain(ram_final) {
        bus.mem[usize::from(word(&e[0]))] = 0;
    }
    for &(addr, _, _) in &bus.log {
        bus.mem[usize::from(addr)] = 0;
    }
    result
}

#[test]
fn harte_single_step_tests() {
    let Some(dir) = std::env::var_os("HARTE_DIR").map(PathBuf::from) else {
        eprintln!("skipped: HARTE_DIR is not set (run scripts/fetch-harte.sh)");
        return;
    };
    if !dir.is_dir() {
        eprintln!("skipped: {} is not a directory", dir.display());
        return;
    }
    let only: Option<Vec<u8>> = std::env::var("HARTE_OPS").ok().map(|s| {
        s.split(',')
            .map(|h| u8::from_str_radix(h.trim(), 16).expect("HARTE_OPS takes hex opcodes"))
            .collect()
    });

    let mut bus = TestBus {
        mem: vec![0; 0x1_0000],
        log: Vec::with_capacity(16),
    };
    let mut failed = Vec::new();
    let mut missing = Vec::new();
    for op in 0..=255u8 {
        if only.as_ref().is_some_and(|o| !o.contains(&op)) {
            continue;
        }
        let path = dir.join(format!("{op:02x}.json"));
        let Ok(text) = std::fs::read_to_string(&path) else {
            missing.push(format!("{op:02x}"));
            continue;
        };
        let cases: Vec<Value> = serde_json::from_str(&text).expect("valid JSON");
        let mut bad = 0;
        let mut first = None;
        for case in &cases {
            if let Err(e) = run_case(case, &mut bus) {
                bad += 1;
                first.get_or_insert_with(|| format!("{}: {e}", case["name"]));
            }
        }
        if let Some(first) = first {
            failed.push(format!(
                "{op:02x}: {bad}/{} failed; first {first}",
                cases.len()
            ));
        }
    }
    assert!(
        missing.is_empty(),
        "missing opcode files: {}",
        missing.join(" ")
    );
    assert!(
        failed.is_empty(),
        "{} opcode(s) failed:\n{}",
        failed.len(),
        failed.join("\n")
    );
}
