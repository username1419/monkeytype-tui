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
/// Reference:
/// * https://github.com/v8/src/base/utils/random-number-generator.h
/// * https://github.com/v8/src/base/utils/random-number-generator.cc
pub(crate) struct Random {
    state_1: u64,
    state_2: u64,
}

impl Random {
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

    fn from_seed(seed: i64) -> (u64, u64) {
        let state_1 = murmur_hash3(u64::from_be_bytes(seed.to_be_bytes()));
        let state_2 = murmur_hash3(!state_1);
        assert_ne!(state_1, state_2);
        (state_1, state_2)
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.move_next();
        self.state_1 + self.state_2
    }

    pub(crate) fn next_f64(&mut self) -> f64 {
        let rand = self.next_u64();
        let random_0_to_2_53 = (rand >> 11) as f64;
        random_0_to_2_53 / (1_u64 << 53) as f64
    }

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

fn murmur_hash3(mut h: u64) -> u64 {
    h ^= h >> 33;
    h *= 0xFF51AFD7ED558CCD_u64;
    h ^= h >> 33;
    h *= 0xC4CEB9FE1A85EC53_u64;
    h ^= h >> 33;
    h
}
