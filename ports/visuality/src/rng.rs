//! java.util.Random on a 48-bit LCG, so port logic keeps Java's draw order and
//! modulo/rejection semantics for tests.

const MASK: u64 = (1u64 << 48) - 1;
const MUL: u64 = 0x5DEECE66D;
const ADD: u64 = 0xB;

#[derive(Clone, Debug)]
pub struct JavaRandom {
    seed: u64,
}

impl JavaRandom {
    pub fn new(seed: u64) -> Self {
        JavaRandom { seed: (seed ^ MUL) & MASK }
    }

    fn next_bits(&mut self, bits: u32) -> i32 {
        self.seed = (self.seed.wrapping_mul(MUL).wrapping_add(ADD)) & MASK;
        (self.seed >> (48 - bits)) as i32
    }

    /// java.util.Random.nextInt(bound) including the rejection path.
    pub fn next_int_bounded(&mut self, bound: u32) -> i32 {
        assert!(bound > 0, "bound must be positive");
        if bound & bound.wrapping_neg() == bound {
            (((bound as i64) * (self.next_bits(31) as i64)) >> 31) as i32
        } else {
            let b = bound as i32;
            loop {
                let bits = self.next_bits(31);
                let val = bits % b;
                if bits.wrapping_sub(val).wrapping_add(b - 1) >= 0 {
                    return val;
                }
            }
        }
    }

    pub fn next_f32(&mut self) -> f32 {
        self.next_bits(24) as f32 / (1u32 << 24) as f32
    }

    pub fn next_double(&mut self) -> f64 {
        (((self.next_bits(26) as i64) << 27) + self.next_bits(27) as i64) as f64
            / (1u64 << 53) as f64
    }
}
