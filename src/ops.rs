//! Arithmetic and logic on register values.
//!
//! Nothing here touches the bus, so each operation can be checked on its own.
//! Every function takes the status register by `&mut` and returns the result.

pub(crate) const C: u8 = 0x01;
pub(crate) const Z: u8 = 0x02;
pub(crate) const I: u8 = 0x04;
pub(crate) const D: u8 = 0x08;
pub(crate) const B: u8 = 0x10;
pub(crate) const U: u8 = 0x20;
pub(crate) const V: u8 = 0x40;
pub(crate) const N: u8 = 0x80;

/// The constant `ANE` (`$8B`) and `LXA` (`$AB`) OR into A before masking.
///
/// On real NMOS parts this value varies from chip to chip and with
/// temperature. `$EE` is what Tom Harte's SingleStepTests record and the most
/// commonly observed value.
pub const UNSTABLE_MAGIC: u8 = 0xEE;

pub(crate) fn set(p: &mut u8, flag: u8, on: bool) {
    if on {
        *p |= flag;
    } else {
        *p &= !flag;
    }
}

pub(crate) fn set_nz(p: &mut u8, v: u8) {
    set(p, Z, v == 0);
    set(p, N, v & 0x80 != 0);
}

/// Add with carry. In NMOS decimal mode Z comes from the binary sum, N and V
/// from the result after the low-nibble adjust, C from the final adjust.
pub(crate) fn adc(p: &mut u8, a: u8, m: u8) -> u8 {
    let c = u16::from(*p & C);
    let bin = u16::from(a) + u16::from(m) + c;
    if *p & D == 0 {
        let r = bin as u8;
        set(p, C, bin > 0xff);
        set(p, V, !(a ^ m) & (a ^ r) & 0x80 != 0);
        set_nz(p, r);
        return r;
    }
    let mut lo = u16::from(a & 0x0f) + u16::from(m & 0x0f) + c;
    if lo > 0x09 {
        lo += 0x06;
    }
    let mut hi = u16::from(a >> 4) + u16::from(m >> 4) + u16::from(lo > 0x0f);
    set(p, Z, bin & 0xff == 0);
    let half = ((hi << 4) | (lo & 0x0f)) as u8;
    set(p, N, half & 0x80 != 0);
    set(p, V, !(a ^ m) & (a ^ half) & 0x80 != 0);
    if hi > 0x09 {
        hi += 0x06;
    }
    set(p, C, hi > 0x0f);
    ((hi << 4) | (lo & 0x0f)) as u8
}

/// Subtract with borrow (borrow = !C). On NMOS every flag comes from the
/// binary result, in decimal mode too; only A is decimal-adjusted.
pub(crate) fn sbc(p: &mut u8, a: u8, m: u8) -> u8 {
    let borrow = i16::from(*p & C == 0);
    let bin = i16::from(a) - i16::from(m) - borrow;
    let r = bin as u8;
    set(p, C, bin >= 0);
    set(p, V, (a ^ m) & (a ^ r) & 0x80 != 0);
    set_nz(p, r);
    if *p & D == 0 {
        return r;
    }
    let mut lo = i16::from(a & 0x0f) - i16::from(m & 0x0f) - borrow;
    let mut hi = i16::from(a >> 4) - i16::from(m >> 4);
    if lo < 0 {
        lo -= 0x06;
        hi -= 1;
    }
    if hi < 0 {
        hi -= 0x06;
    }
    ((hi << 4) | (lo & 0x0f)) as u8
}

/// `CMP`/`CPX`/`CPY`: C is "no borrow", N and Z from the difference.
pub(crate) fn compare(p: &mut u8, r: u8, m: u8) {
    set(p, C, r >= m);
    set_nz(p, r.wrapping_sub(m));
}

pub(crate) fn bit(p: &mut u8, a: u8, m: u8) {
    set(p, Z, a & m == 0);
    set(p, N, m & 0x80 != 0);
    set(p, V, m & 0x40 != 0);
}

pub(crate) fn asl(p: &mut u8, v: u8) -> u8 {
    set(p, C, v & 0x80 != 0);
    let r = v << 1;
    set_nz(p, r);
    r
}

pub(crate) fn lsr(p: &mut u8, v: u8) -> u8 {
    set(p, C, v & 0x01 != 0);
    let r = v >> 1;
    set_nz(p, r);
    r
}

pub(crate) fn rol(p: &mut u8, v: u8) -> u8 {
    let r = (v << 1) | (*p & C);
    set(p, C, v & 0x80 != 0);
    set_nz(p, r);
    r
}

pub(crate) fn ror(p: &mut u8, v: u8) -> u8 {
    let r = (v >> 1) | ((*p & C) << 7);
    set(p, C, v & 0x01 != 0);
    set_nz(p, r);
    r
}

/// `ANC`: AND, then C copies N.
pub(crate) fn anc(p: &mut u8, a: u8, m: u8) -> u8 {
    let r = a & m;
    set_nz(p, r);
    set(p, C, r & 0x80 != 0);
    r
}

/// `ALR`: AND, then LSR.
pub(crate) fn alr(p: &mut u8, a: u8, m: u8) -> u8 {
    lsr(p, a & m)
}

/// `ARR`: AND, then ROR, with its own flag rules and an NMOS decimal variant.
pub(crate) fn arr(p: &mut u8, a: u8, m: u8) -> u8 {
    let t = a & m;
    let carry = *p & C;
    let mut r = (t >> 1) | (carry << 7);
    if *p & D == 0 {
        set_nz(p, r);
        set(p, C, r & 0x40 != 0);
        set(p, V, ((r >> 6) ^ (r >> 5)) & 1 != 0);
        return r;
    }
    set(p, N, carry != 0);
    set(p, Z, r == 0);
    set(p, V, (t ^ r) & 0x40 != 0);
    let (hi, lo) = (t >> 4, t & 0x0f);
    if lo + (lo & 1) > 5 {
        r = (r & 0xf0) | (r.wrapping_add(6) & 0x0f);
    }
    let c = hi + (hi & 1) > 5;
    set(p, C, c);
    if c {
        r = r.wrapping_add(0x60);
    }
    r
}

/// `SBX` (also called AXS): X = (A & X) - imm, flags like CMP, no decimal.
pub(crate) fn sbx(p: &mut u8, a: u8, x: u8, m: u8) -> u8 {
    let t = a & x;
    set(p, C, t >= m);
    let r = t.wrapping_sub(m);
    set_nz(p, r);
    r
}

/// `ANE` (also called XAA): A = (A | magic) & X & imm.
pub(crate) fn ane(p: &mut u8, a: u8, x: u8, m: u8) -> u8 {
    let r = (a | UNSTABLE_MAGIC) & x & m;
    set_nz(p, r);
    r
}

/// `LXA`: A = X = (A | magic) & imm.
pub(crate) fn lxa(p: &mut u8, a: u8, m: u8) -> u8 {
    let r = (a | UNSTABLE_MAGIC) & m;
    set_nz(p, r);
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adc_binary_sets_carry_and_overflow() {
        let mut p = 0;
        assert_eq!(adc(&mut p, 0x50, 0x50), 0xa0);
        assert_eq!(p & (C | V | N | Z), V | N);
        let mut p = 0;
        assert_eq!(adc(&mut p, 0xff, 0x01), 0x00);
        assert_eq!(p & (C | V | N | Z), C | Z);
    }

    #[test]
    fn adc_decimal_carries_out() {
        let mut p = D | C;
        assert_eq!(adc(&mut p, 0x58, 0x46), 0x05);
        assert_ne!(p & C, 0);
    }

    #[test]
    fn sbc_binary_uses_the_borrow() {
        let mut p = C;
        assert_eq!(sbc(&mut p, 0x00, 0x01), 0xff);
        assert_eq!(p & (C | N), N);
        let mut p = 0;
        assert_eq!(sbc(&mut p, 0x10, 0x05), 0x0a);
        assert_ne!(p & C, 0);
    }

    #[test]
    fn sbc_decimal() {
        let mut p = D | C;
        assert_eq!(sbc(&mut p, 0x46, 0x12), 0x34);
        assert_ne!(p & C, 0);
        let mut p = D;
        assert_eq!(sbc(&mut p, 0x40, 0x13), 0x26);
    }

    #[test]
    fn rotates_go_through_carry() {
        let mut p = C;
        assert_eq!(ror(&mut p, 0x02), 0x81);
        assert_eq!(p & C, 0);
        let mut p = 0;
        assert_eq!(rol(&mut p, 0x80), 0x00);
        assert_eq!(p & (C | Z), C | Z);
    }

    #[test]
    fn compare_sets_carry_when_no_borrow() {
        let mut p = 0;
        compare(&mut p, 0x10, 0x10);
        assert_eq!(p & (C | Z | N), C | Z);
    }

    #[test]
    fn arr_binary_flags_come_from_bits_6_and_5() {
        let mut p = C;
        assert_eq!(arr(&mut p, 0xff, 0xff), 0xff);
        assert_eq!(p & (C | V | N), C | N);
    }

    #[test]
    fn sbx_subtracts_from_a_and_x() {
        let mut p = 0;
        assert_eq!(sbx(&mut p, 0xf0, 0x3c, 0x10), 0x20);
        assert_ne!(p & C, 0);
    }

    #[test]
    fn ane_and_lxa_use_the_magic_constant() {
        let mut p = 0;
        assert_eq!(ane(&mut p, 0x00, 0xff, 0xff), UNSTABLE_MAGIC);
        assert_eq!(lxa(&mut p, 0x00, 0x0f), 0x0e);
    }
}
