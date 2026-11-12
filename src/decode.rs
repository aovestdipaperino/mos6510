//! Opcode decoding: each of the 256 opcodes as an operation and an
//! addressing mode. No logic lives here beyond the table and the access kind.

/// What an instruction does once it has its operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Op {
    // Read: the operand is consumed.
    Lda,
    Ldx,
    Ldy,
    Lax,
    Ora,
    And,
    Eor,
    Adc,
    Sbc,
    Cmp,
    Cpx,
    Cpy,
    Bit,
    Anc,
    Alr,
    Arr,
    Sbx,
    Ane,
    Lxa,
    Las,
    Nop,
    // Write: a value is stored.
    Sta,
    Stx,
    Sty,
    Sax,
    Sha,
    Shx,
    Shy,
    Tas,
    // Read-modify-write (on A when the mode is Acc).
    Asl,
    Lsr,
    Rol,
    Ror,
    Inc,
    Dec,
    Slo,
    Rla,
    Sre,
    Rra,
    Dcp,
    Isc,
    // Implied, two cycles.
    Tax,
    Tay,
    Txa,
    Tya,
    Tsx,
    Txs,
    Inx,
    Iny,
    Dex,
    Dey,
    Clc,
    Sec,
    Cli,
    Sei,
    Clv,
    Cld,
    Sed,
    // Instructions with their own cycle sequences.
    Brk,
    Jsr,
    Rts,
    Rti,
    Jmp,
    JmpInd,
    Pha,
    Php,
    Pla,
    Plp,
    Jam,
    Bpl,
    Bmi,
    Bvc,
    Bvs,
    Bcc,
    Bcs,
    Bne,
    Beq,
}

/// How an instruction reaches its operand. `Imp` is also used by the
/// instructions with their own sequences, which ignore it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Imp,
    Acc,
    Imm,
    Zp,
    Zpx,
    Zpy,
    Abs,
    Abx,
    Aby,
    Izx,
    Izy,
}

/// The bus pattern an operation needs at its effective address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Read,
    Write,
    Rmw,
    Other,
}

impl Op {
    pub(crate) const fn kind(self) -> Kind {
        use Op::*;
        match self {
            Lda | Ldx | Ldy | Lax | Ora | And | Eor | Adc | Sbc | Cmp | Cpx | Cpy | Bit | Anc
            | Alr | Arr | Sbx | Ane | Lxa | Las | Nop => Kind::Read,
            Sta | Stx | Sty | Sax | Sha | Shx | Shy | Tas => Kind::Write,
            Asl | Lsr | Rol | Ror | Inc | Dec | Slo | Rla | Sre | Rra | Dcp | Isc => Kind::Rmw,
            _ => Kind::Other,
        }
    }
}

use Mode::*;
use Op::*;

#[rustfmt::skip]
pub(crate) const TABLE: [(Op, Mode); 256] = [
    // 0x00
    (Brk, Imp), (Ora, Izx), (Jam, Imp), (Slo, Izx), (Nop, Zp),  (Ora, Zp),  (Asl, Zp),  (Slo, Zp),
    (Php, Imp), (Ora, Imm), (Asl, Acc), (Anc, Imm), (Nop, Abs), (Ora, Abs), (Asl, Abs), (Slo, Abs),
    // 0x10
    (Bpl, Imp), (Ora, Izy), (Jam, Imp), (Slo, Izy), (Nop, Zpx), (Ora, Zpx), (Asl, Zpx), (Slo, Zpx),
    (Clc, Imp), (Ora, Aby), (Nop, Imp), (Slo, Aby), (Nop, Abx), (Ora, Abx), (Asl, Abx), (Slo, Abx),
    // 0x20
    (Jsr, Imp), (And, Izx), (Jam, Imp), (Rla, Izx), (Bit, Zp),  (And, Zp),  (Rol, Zp),  (Rla, Zp),
    (Plp, Imp), (And, Imm), (Rol, Acc), (Anc, Imm), (Bit, Abs), (And, Abs), (Rol, Abs), (Rla, Abs),
    // 0x30
    (Bmi, Imp), (And, Izy), (Jam, Imp), (Rla, Izy), (Nop, Zpx), (And, Zpx), (Rol, Zpx), (Rla, Zpx),
    (Sec, Imp), (And, Aby), (Nop, Imp), (Rla, Aby), (Nop, Abx), (And, Abx), (Rol, Abx), (Rla, Abx),
    // 0x40
    (Rti, Imp), (Eor, Izx), (Jam, Imp), (Sre, Izx), (Nop, Zp),  (Eor, Zp),  (Lsr, Zp),  (Sre, Zp),
    (Pha, Imp), (Eor, Imm), (Lsr, Acc), (Alr, Imm), (Jmp, Imp), (Eor, Abs), (Lsr, Abs), (Sre, Abs),
    // 0x50
    (Bvc, Imp), (Eor, Izy), (Jam, Imp), (Sre, Izy), (Nop, Zpx), (Eor, Zpx), (Lsr, Zpx), (Sre, Zpx),
    (Cli, Imp), (Eor, Aby), (Nop, Imp), (Sre, Aby), (Nop, Abx), (Eor, Abx), (Lsr, Abx), (Sre, Abx),
    // 0x60
    (Rts, Imp), (Adc, Izx), (Jam, Imp), (Rra, Izx), (Nop, Zp),  (Adc, Zp),  (Ror, Zp),  (Rra, Zp),
    (Pla, Imp), (Adc, Imm), (Ror, Acc), (Arr, Imm), (JmpInd, Imp), (Adc, Abs), (Ror, Abs), (Rra, Abs),
    // 0x70
    (Bvs, Imp), (Adc, Izy), (Jam, Imp), (Rra, Izy), (Nop, Zpx), (Adc, Zpx), (Ror, Zpx), (Rra, Zpx),
    (Sei, Imp), (Adc, Aby), (Nop, Imp), (Rra, Aby), (Nop, Abx), (Adc, Abx), (Ror, Abx), (Rra, Abx),
    // 0x80
    (Nop, Imm), (Sta, Izx), (Nop, Imm), (Sax, Izx), (Sty, Zp),  (Sta, Zp),  (Stx, Zp),  (Sax, Zp),
    (Dey, Imp), (Nop, Imm), (Txa, Imp), (Ane, Imm), (Sty, Abs), (Sta, Abs), (Stx, Abs), (Sax, Abs),
    // 0x90
    (Bcc, Imp), (Sta, Izy), (Jam, Imp), (Sha, Izy), (Sty, Zpx), (Sta, Zpx), (Stx, Zpy), (Sax, Zpy),
    (Tya, Imp), (Sta, Aby), (Txs, Imp), (Tas, Aby), (Shy, Abx), (Sta, Abx), (Shx, Aby), (Sha, Aby),
    // 0xa0
    (Ldy, Imm), (Lda, Izx), (Ldx, Imm), (Lax, Izx), (Ldy, Zp),  (Lda, Zp),  (Ldx, Zp),  (Lax, Zp),
    (Tay, Imp), (Lda, Imm), (Tax, Imp), (Lxa, Imm), (Ldy, Abs), (Lda, Abs), (Ldx, Abs), (Lax, Abs),
    // 0xb0
    (Bcs, Imp), (Lda, Izy), (Jam, Imp), (Lax, Izy), (Ldy, Zpx), (Lda, Zpx), (Ldx, Zpy), (Lax, Zpy),
    (Clv, Imp), (Lda, Aby), (Tsx, Imp), (Las, Aby), (Ldy, Abx), (Lda, Abx), (Ldx, Aby), (Lax, Aby),
    // 0xc0
    (Cpy, Imm), (Cmp, Izx), (Nop, Imm), (Dcp, Izx), (Cpy, Zp),  (Cmp, Zp),  (Dec, Zp),  (Dcp, Zp),
    (Iny, Imp), (Cmp, Imm), (Dex, Imp), (Sbx, Imm), (Cpy, Abs), (Cmp, Abs), (Dec, Abs), (Dcp, Abs),
    // 0xd0
    (Bne, Imp), (Cmp, Izy), (Jam, Imp), (Dcp, Izy), (Nop, Zpx), (Cmp, Zpx), (Dec, Zpx), (Dcp, Zpx),
    (Cld, Imp), (Cmp, Aby), (Nop, Imp), (Dcp, Aby), (Nop, Abx), (Cmp, Abx), (Dec, Abx), (Dcp, Abx),
    // 0xe0
    (Cpx, Imm), (Sbc, Izx), (Nop, Imm), (Isc, Izx), (Cpx, Zp),  (Sbc, Zp),  (Inc, Zp),  (Isc, Zp),
    (Inx, Imp), (Sbc, Imm), (Nop, Imp), (Sbc, Imm), (Cpx, Abs), (Sbc, Abs), (Inc, Abs), (Isc, Abs),
    // 0xf0
    (Beq, Imp), (Sbc, Izy), (Jam, Imp), (Isc, Izy), (Nop, Zpx), (Sbc, Zpx), (Inc, Zpx), (Isc, Zpx),
    (Sed, Imp), (Sbc, Aby), (Nop, Imp), (Isc, Aby), (Nop, Abx), (Sbc, Abx), (Inc, Abx), (Isc, Abx),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spot_checks() {
        assert_eq!(TABLE[0xa9], (Lda, Imm));
        assert_eq!(TABLE[0x6c], (JmpInd, Imp));
        assert_eq!(TABLE[0xeb], (Sbc, Imm));
        assert_eq!(TABLE[0x9e], (Shx, Aby));
        assert_eq!(TABLE[0xb6], (Ldx, Zpy));
    }

    #[test]
    fn twelve_jams() {
        assert_eq!(TABLE.iter().filter(|(op, _)| *op == Jam).count(), 12);
    }

    #[test]
    fn memory_operations_have_memory_modes() {
        for (i, (op, mode)) in TABLE.iter().enumerate() {
            match op.kind() {
                Kind::Read => assert!(*mode != Acc, "{i:02x}"),
                Kind::Write => assert!(!matches!(mode, Imp | Acc | Imm), "{i:02x}"),
                Kind::Rmw => assert!(!matches!(mode, Imp | Imm), "{i:02x}"),
                Kind::Other => assert_eq!(*mode, Imp, "{i:02x}"),
            }
        }
    }
}
