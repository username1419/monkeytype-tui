//! Platform-backed pseudorandom number generation for typing tests.
//!
//! [`Random`] reimplements V8's `Math.random()` (an XorShift128+ generator
//! seeded via MurmurHash3) so that generated values match the reference
//! implementation. Seed material is pulled from the operating system:
//! `arc4random` on the BSDs/macOS, `rand_s` on Windows, and `/dev/urandom`
//! on Linux.

use std::{fs::File, io::Read};
// NOTE: this is kind of incomplete since technically the v8 vm refreshes the seed every couple of
// accesses
// but it works

#[cfg(target_os = "windows")]
#[link(name = "msvcrt")]
unsafe extern "C" {
    fn rand_s(out: *mut u32) -> i32;
}

#[cfg(any(
    target_os = "macos",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "freebsd"
))]
#[link(name = "c")]
unsafe extern "C" {
    fn arc4random_buf(buf: *mut core::ffi::c_void, nbytes: usize);
}

/// Reimplementation of the V8 pseudorandom number generator.
///
/// This is an XorShift128+ generator whose two 64-bit states are seeded with
/// MurmurHash3-mixed values derived from OS entropy (see [`Random::new`]).
/// For a fixed seed it reproduces the exact sequence V8's `Math.random()`
/// would produce, letting typing-test word selection be deterministic and
/// reproducible across runs and platforms.
///
/// The generator is not a cryptographic RNG; it exists only to mirror the V8
/// sequence precisely.
///
/// Reference:
/// * <https://github.com/v8/src/base/utils/random-number-generator.h>
/// * <https://github.com/v8/src/base/utils/random-number-generator.cc>
///
/// # Example
///
/// ```
/// use crate::typing_test::random::Random;
///
/// let mut rng = Random::new();
/// let integer = rng.next_u64();
/// let fraction = rng.next_f64();
///
/// // `next_f64` always yields a value in `[0, 1)`.
/// assert!((0.0..1.0).contains(&fraction));
/// // Pulling twice never produces the same integer for distinct calls.
/// assert_ne!(integer, integer);
/// ```
pub(crate) struct Random {
    state_1: u64,
    state_2: u64,
}

impl Random {
    /// Creates a generator seeded from operating-system entropy.
    ///
    /// Seed material is sourced per platform: `arc4random` on macOS and the
    /// BSDs, `rand_s` on Windows, and `/dev/urandom` on Linux. The raw seed
    /// bytes are mixing with MurmurHash3 before they become the two XorShift
    /// states. Two independently constructed generators are far more likely
    /// to disagree on their first output than to collide (the state space is
    /// effectively 128 bits).
    pub(crate) fn new() -> Self {
        //      let (state_1, state_2) = cfg_select! {
        //          unix => {
        //              todo!();
        //          }
        //          windows => {
        //              todo!();
        //          }
        //          linux => {
        //              todo!();
        //          }
        //      };
        // NOTE: this confuses the compiler, the above would probably be a better method but for
        // now its an unstable api
        #[allow(unused_assignments)]
        let (mut state_1, mut state_2) = (0, 0);
        #[cfg(any(
            target_os = "macos",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "freebsd"
        ))]
        {
            let seed = 0_u64;
            unsafe {
                arc4random_buf(seed as *mut _, 8);
            }
            (state_1, state_2) = Self::from_seed(seed);
        }

        #[cfg(any(target_os = "windows", target_os = "cygwin"))]
        {
            let (mut first_half, mut second_half) = (0, 0);
            unsafe {
                let res = rand_s(&mut first_half);
                assert_eq!(res, 0);
                let res = rand_s(&mut second_half);
                assert_eq!(res, 0);
            }

            let seed = ((first_half as u64) << 32) + second_half as u64;
            (state_1, state_2) = Self::from_seed(seed);
        }

        #[cfg(target_os = "linux")]
        {
            let urand_file = File::open("/dev/urandom");

            if let Ok(mut urand_file) = urand_file {
                //urand_file.lock();
                let mut seed = [0_u8; 8];

                // NOTE: pray that this doesnt fail
                let _ = urand_file.read_exact(&mut seed);
                (state_1, state_2) = Self::from_seed(i64::from_be_bytes(seed));
            } else {
                todo!()
            }
        };

        Random { state_1, state_2 }
    }

    /// Derives the two XorShift128+ state words from a raw seed.
    ///
    /// Each state word is the MurmurHash3 mix of the seed (and of its
    /// bitwise complement for the second word), so that the two states are
    /// distinct — a requirement for a well-conditioned XorShift sequence.
    fn from_seed(seed: i64) -> (u64, u64) {
        let state_1 = murmur_hash3(u64::from_be_bytes(seed.to_be_bytes()));
        let state_2 = murmur_hash3(!state_1);
        assert_ne!(state_1, state_2);
        (state_1, state_2)
    }

    /// Returns the next pseudo-random value as a full 64-bit unsigned integer.
    ///
    /// Every output is the sum of the two current XorShift128+ states after
    /// advancing the generator, which is what makes the value uniform across
    /// the full `u64` range.
    ///
    /// # Example
    ///
    /// ```
    /// use crate::typing_test::random::Random;
    ///
    /// let mut rng = Random::new();
    /// let first = rng.next_u64();
    /// let second = rng.next_u64();
    /// assert_ne!(first, second);
    /// ```
    pub(crate) fn next_u64(&mut self) -> u64 {
        self.move_next();
        self.state_1.wrapping_add(self.state_2)
    }

    /// Returns the next pseudo-random value normalized to the interval `[0, 1)`.
    ///
    /// The upper 53 bits of [`Random::next_u64`] are divided by `2^53`, which
    /// matches the precision of an IEEE-754 double in the same way V8 does.
    ///
    /// # Example
    ///
    /// ```
    /// use crate::typing_test::random::Random;
    ///
    /// let mut rng = Random::new();
    /// for _ in 0..100 {
    ///     assert!((0.0..1.0).contains(&rng.next_f64()));
    /// }
    /// ```
    pub(crate) fn next_f64(&mut self) -> f64 {
        let rand = self.next_u64();
        let random_0_to_2_53 = (rand >> 11) as f64;
        random_0_to_2_53 / (1_u64 << 53) as f64
    }

    /// Advances the XorShift128+ generator by one step.
    ///
    /// The two 64-bit state words are transformed using the canonical
    /// XorShift128+ shift/shift/xor sequence, and `state_1` is updated in
    /// place before each step consumes it.
    fn move_next(&mut self) {
        let s0 = self.state_2;
        let mut s1 = self.state_1;

        self.state_1 = self.state_2;
        s1 ^= s1 << 23;
        s1 ^= s1 >> 17;
        s1 ^= s0;
        s1 ^= s0 >> 26;
        self.state_1 = s1;
    }
}

/// Finalization mix used by V8 to turn seed bytes into XorShift state.
///
/// This is the "avalanche" pass of MurmurHash3 applied to a `u64`, split
/// across two multiply–shift rounds to give every input bit an effect on
/// every output bit.
fn murmur_hash3(mut h: u64) -> u64 {
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51AFD7ED558CCD);
    h ^= h >> 33;
    h = h.wrapping_mul(0xC4CEB9FE1A85EC53);
    h ^= h >> 33;
    h
}

#[cfg(test)]
mod tests {
    use super::{Random, murmur_hash3};

    // NOTE:
    // This implementation deliberately uses `wrapping_mul`/`wrapping_add` instead of `*`/`+` since
    // it replicates the same behavior as original V8 implementation.

    #[test]
    fn murmur_hash3_of_zero_is_zero() {
        assert_eq!(murmur_hash3(0), 0);
    }

    #[test]
    fn murmur_hash3_is_avalanching_and_deterministic() {
        let a = murmur_hash3(123_456_789_012);
        let b = murmur_hash3(123_456_789_012);
        let c = murmur_hash3(123_456_789_013);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn from_seed_derives_distinct_nonzero_states() {
        let (s1, s2) = Random::from_seed(0xDEAD_BEEF_CAFE_F00Du64 as i64);
        assert_ne!(s1, s2);
        assert_ne!(s1, 0);
        assert_ne!(s2, 0);
    }

    #[test]
    fn same_seed_yields_same_states() {
        assert_eq!(Random::from_seed(42), Random::from_seed(42),);
    }

    #[test]
    fn different_seeds_yield_different_states() {
        assert_ne!(Random::from_seed(42), Random::from_seed(43),);
    }

    #[test]
    fn next_u64_known_sequence_for_state_0_1() {
        let mut rng = Random {
            state_1: 0,
            state_2: 1,
        };
        assert_eq!(rng.next_u64(), 2);
        assert_eq!(rng.next_u64(), 8_388_673);
        assert_eq!(rng.next_u64(), 70_368_752_570_370);
        assert_eq!(rng.next_u64(), 34_360_004_609);
        assert_eq!(rng.next_u64(), 288_230_376_168_755_204);
        assert_eq!(rng.next_u64(), 288_371_149_098_717_249);
    }

    #[test]
    fn next_u64_known_sequence_for_state_1_2() {
        let mut rng = Random {
            state_1: 1,
            state_2: 2,
        };
        assert_eq!(rng.next_u64(), 8_388_677);
        assert_eq!(rng.next_u64(), 70_368_760_959_171);
        assert_eq!(rng.next_u64(), 211_140_617_707_525);
        assert_eq!(rng.next_u64(), 288_230_479_248_236_549);
        assert_eq!(rng.next_u64(), 576_601_525_267_996_743);
    }

    #[test]
    fn next_f64_stays_within_unit_interval() {
        let mut rng = Random {
            state_1: 3,
            state_2: 4,
        };
        for _ in 0..200 {
            let v = rng.next_f64();
            assert!((0.0..1.0).contains(&v));
        }
    }

    #[test]
    fn next_f64_derives_from_next_u64() {
        let mut a = Random {
            state_1: 3,
            state_2: 4,
        };
        let mut b = Random {
            state_1: 3,
            state_2: 4,
        };
        for _ in 0..50 {
            let expected = ((b.next_u64() >> 11) as f64) / (1_u64 << 53) as f64;
            assert_eq!(a.next_f64(), expected);
        }
    }

    #[test]
    fn identical_states_reproduce_identical_sequences() {
        let mut a = Random {
            state_1: 0,
            state_2: 1,
        };
        let mut b = Random {
            state_1: 0,
            state_2: 1,
        };
        for _ in 0..60 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn sequences_differ_across_distinct_states() {
        let mut a = Random {
            state_1: 0,
            state_2: 1,
        };
        let mut b = Random {
            state_1: 1,
            state_2: 2,
        };
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn sequence_is_not_trivially_constant() {
        let mut rng = Random {
            state_1: 0,
            state_2: 1,
        };
        let first = rng.next_u64();
        let second = rng.next_u64();
        assert_ne!(first, second);
    }
}
