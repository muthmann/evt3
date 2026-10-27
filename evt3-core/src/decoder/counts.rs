//! Scalar and SIMD output sizing, independent of optional thread execution.
use super::*;

#[derive(Default)]
pub(super) struct Counts {
    pub(super) cd: usize,
    pub(super) triggers: usize,
    pub(super) high: bool,
}

pub(super) fn scalar_counts(bytes: &[u8]) -> Counts {
    let mut counts = Counts::default();
    for word in bytes.chunks_exact(2) {
        let word = u16::from_le_bytes([word[0], word[1]]);
        counts.cd += match word >> 12 {
            2 => 1,
            4 => (word & 0x0fff).count_ones() as usize,
            5 => (word & 0x00ff).count_ones() as usize,
            _ => 0,
        };
        counts.triggers += usize::from(word >> 12 == 10);
        counts.high |= word >> 12 == 8;
    }
    counts
}

#[cfg(all(
    target_arch = "aarch64",
    target_feature = "neon",
    target_endian = "little"
))]
#[inline]
pub(super) fn block_counts(bytes: &[u8; 64]) -> Counts {
    use std::arch::aarch64::*;
    // SAFETY: NEON is part of this compilation target. The load reads exactly
    // four disjoint sixteen-byte ranges in the initialized array. vld1q allows
    // unaligned input. Four populations per byte lane fit in u8 (at most 32);
    // widen before the final sum, whose maximum CD count is 32 * 12 = 384.
    unsafe {
        let mut populations = vdupq_n_u8(0);
        let mut singles = vdupq_n_u16(0);
        let mut triggers = vdupq_n_u16(0);
        let mut high = vdupq_n_u16(0);
        let ones = vdupq_n_u16(1);
        for offset in [0, 16, 32, 48] {
            let words = vld1q_u16(bytes.as_ptr().add(offset).cast::<u16>());
            let tags = vshrq_n_u16::<12>(words);
            let masks = vorrq_u16(
                vandq_u16(vceqq_u16(tags, vdupq_n_u16(4)), vdupq_n_u16(0x0fff)),
                vandq_u16(vceqq_u16(tags, vdupq_n_u16(5)), vdupq_n_u16(0x00ff)),
            );
            populations = vaddq_u8(
                populations,
                vcntq_u8(vreinterpretq_u8_u16(vandq_u16(words, masks))),
            );
            singles = vaddq_u16(singles, vandq_u16(vceqq_u16(tags, vdupq_n_u16(2)), ones));
            triggers = vaddq_u16(triggers, vandq_u16(vceqq_u16(tags, vdupq_n_u16(10)), ones));
            high = vorrq_u16(high, vceqq_u16(tags, vdupq_n_u16(8)));
        }
        Counts {
            cd: vaddvq_u16(vpaddlq_u8(populations)) as usize + vaddvq_u16(singles) as usize,
            triggers: vaddvq_u16(triggers) as usize,
            high: vmaxvq_u16(high) != 0,
        }
    }
}

#[cfg(not(all(
    target_arch = "aarch64",
    target_feature = "neon",
    target_endian = "little"
)))]
#[inline]
pub(super) fn block_counts(bytes: &[u8; 64]) -> Counts {
    scalar_counts(bytes)
}

impl Evt3Decoder {
    pub(crate) fn output_counts(&self, bytes: &[u8]) -> (usize, usize) {
        let mut remaining = bytes;
        let mut total = Counts::default();
        if let (Some(pending), Some((&next, rest))) = (self.pending_byte, bytes.split_first()) {
            total = scalar_counts(&[pending, next]);
            remaining = rest;
        }
        let mut blocks = remaining.chunks_exact(64);
        for block in &mut blocks {
            let counts = block_counts(block.try_into().unwrap());
            total.cd = total
                .cd
                .checked_add(counts.cd)
                .expect("event count overflow");
            total.triggers = total
                .triggers
                .checked_add(counts.triggers)
                .expect("trigger count overflow");
        }
        let tail = scalar_counts(blocks.remainder());
        (
            total.cd.checked_add(tail.cd).expect("event count overflow"),
            total
                .triggers
                .checked_add(tail.triggers)
                .expect("trigger count overflow"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vectorized_counts_match_scalar_for_every_word_value() {
        for first in (0..=u16::MAX).step_by(32) {
            let mut bytes = [0_u8; 64];
            for (i, pair) in bytes.chunks_exact_mut(2).enumerate() {
                pair.copy_from_slice(&(first + i as u16).to_le_bytes());
            }
            let expected = scalar_counts(&bytes);
            let actual = block_counts(&bytes);
            assert_eq!(
                (actual.cd, actual.triggers, actual.high),
                (expected.cd, expected.triggers, expected.high)
            );
        }
    }
}
