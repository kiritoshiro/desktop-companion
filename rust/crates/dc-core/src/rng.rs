//! A bit-exact port of CPython's `random.Random` (MT19937).
//!
//! The Python simulation's behaviour depends on the order and kind of every
//! draw, and the existing determinism tests replay seeded runs. Matching
//! CPython exactly lets those traces act as a parity oracle for the port.
//! Every method documents the CPython function it mirrors; keep draw counts
//! identical when porting call sites (a `choice` of one element still
//! consumes a word, for example).

use sha2::{Digest, Sha512};

const N: usize = 624;
const M: usize = 397;
const MATRIX_A: u32 = 0x9908_b0df;
const UPPER_MASK: u32 = 0x8000_0000;
const LOWER_MASK: u32 = 0x7fff_ffff;

const TWOPI: f64 = std::f64::consts::TAU;

/// `NV_MAGICCONST = 4 * exp(-0.5) / sqrt(2.0)`
fn nv_magicconst() -> f64 {
    4.0 * (-0.5f64).exp() / 2.0f64.sqrt()
}

#[derive(Clone)]
pub struct PyRandom {
    mt: [u32; N],
    index: usize,
    gauss_next: Option<f64>,
}

impl std::fmt::Debug for PyRandom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PyRandom").field("index", &self.index).finish_non_exhaustive()
    }
}

impl Default for PyRandom {
    fn default() -> Self {
        Self::from_entropy()
    }
}

impl PyRandom {
    fn blank() -> Self {
        Self { mt: [0; N], index: N + 1, gauss_next: None }
    }

    /// `random.Random(n)` for a non-negative or negative integer seed.
    pub fn from_int(seed: i128) -> Self {
        let mut r = Self::blank();
        r.seed_int(seed);
        r
    }

    pub fn from_u64(seed: u64) -> Self {
        Self::from_int(seed as i128)
    }

    /// `random.Random("text")` (seed version 2: UTF-8 bytes + SHA-512).
    pub fn from_str_seed(seed: &str) -> Self {
        let mut r = Self::blank();
        r.seed_str(seed);
        r
    }

    /// `random.Random()` with no seed. Not reproducible.
    pub fn from_entropy() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let addr = &nanos as *const _ as usize as u128;
        let mut r = Self::blank();
        r.init_by_array(&words_from_be_bytes(&(nanos ^ (addr << 64)).to_be_bytes()));
        r
    }

    pub fn seed_int(&mut self, seed: i128) {
        let a = seed.unsigned_abs();
        self.init_by_array(&words_from_be_bytes(&a.to_be_bytes()));
        self.gauss_next = None;
    }

    pub fn seed_str(&mut self, seed: &str) {
        let mut bytes = seed.as_bytes().to_vec();
        let digest = Sha512::digest(seed.as_bytes());
        bytes.extend_from_slice(&digest);
        self.init_by_array(&words_from_be_bytes(&bytes));
        self.gauss_next = None;
    }

    fn init_genrand(&mut self, s: u32) {
        self.mt[0] = s;
        for i in 1..N {
            let prev = self.mt[i - 1];
            self.mt[i] = 1_812_433_253u32.wrapping_mul(prev ^ (prev >> 30)).wrapping_add(i as u32);
        }
        self.index = N;
    }

    fn init_by_array(&mut self, key: &[u32]) {
        self.init_genrand(19_650_218);
        let mut i = 1usize;
        let mut j = 0usize;
        let len = key.len();
        let mut k = N.max(len);
        while k > 0 {
            let prev = self.mt[i - 1];
            self.mt[i] = (self.mt[i] ^ (prev ^ (prev >> 30)).wrapping_mul(1_664_525))
                .wrapping_add(key[j])
                .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= N {
                self.mt[0] = self.mt[N - 1];
                i = 1;
            }
            if j >= len {
                j = 0;
            }
            k -= 1;
        }
        k = N - 1;
        while k > 0 {
            let prev = self.mt[i - 1];
            self.mt[i] = (self.mt[i] ^ (prev ^ (prev >> 30)).wrapping_mul(1_566_083_941)).wrapping_sub(i as u32);
            i += 1;
            if i >= N {
                self.mt[0] = self.mt[N - 1];
                i = 1;
            }
            k -= 1;
        }
        self.mt[0] = 0x8000_0000;
        self.index = N;
    }

    /// `genrand_uint32`
    pub fn next_u32(&mut self) -> u32 {
        if self.index >= N {
            for kk in 0..N {
                let y = (self.mt[kk] & UPPER_MASK) | (self.mt[(kk + 1) % N] & LOWER_MASK);
                let mag = if y & 1 != 0 { MATRIX_A } else { 0 };
                self.mt[kk] = self.mt[(kk + M) % N] ^ (y >> 1) ^ mag;
            }
            self.index = 0;
        }
        let mut y = self.mt[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }

    /// `random()`: a float in [0, 1) with 53 random bits.
    pub fn random(&mut self) -> f64 {
        let a = (self.next_u32() >> 5) as f64;
        let b = (self.next_u32() >> 6) as f64;
        (a * 67_108_864.0 + b) * (1.0 / 9_007_199_254_740_992.0)
    }

    /// `getrandbits(k)` for `0 <= k <= 64`.
    pub fn getrandbits(&mut self, k: u32) -> u64 {
        assert!(k <= 64, "getrandbits supports up to 64 bits here");
        if k == 0 {
            return 0;
        }
        if k <= 32 {
            return (self.next_u32() >> (32 - k)) as u64;
        }
        // Little-endian 32-bit words, the last one truncated to the remaining bits.
        let low = self.next_u32() as u64;
        let rest = k - 32;
        let high = (self.next_u32() >> (32 - rest)) as u64;
        low | (high << 32)
    }

    /// `_randbelow_with_getrandbits(n)`; returns 0 for `n == 0` without drawing.
    pub fn randbelow(&mut self, n: u64) -> u64 {
        if n == 0 {
            return 0;
        }
        let k = 64 - n.leading_zeros();
        let mut r = self.getrandbits(k);
        while r >= n {
            r = self.getrandbits(k);
        }
        r
    }

    /// `randrange(stop)`
    pub fn randrange(&mut self, stop: i64) -> i64 {
        assert!(stop > 0, "empty range for randrange()");
        self.randbelow(stop as u64) as i64
    }

    /// `randrange(start, stop)` with step 1.
    pub fn randrange2(&mut self, start: i64, stop: i64) -> i64 {
        let width = stop - start;
        assert!(width > 0, "empty range for randrange({start}, {stop})");
        start + self.randbelow(width as u64) as i64
    }

    /// `randrange(start, stop, step)`
    pub fn randrange_step(&mut self, start: i64, stop: i64, step: i64) -> i64 {
        if step == 1 {
            return self.randrange2(start, stop);
        }
        assert!(step != 0, "zero step for randrange()");
        let width = stop - start;
        let n = if step > 0 { (width + step - 1) / step } else { (width + step + 1) / step };
        assert!(n > 0, "empty range for randrange()");
        start + step * self.randbelow(n as u64) as i64
    }

    /// `randint(a, b)`, inclusive.
    pub fn randint(&mut self, a: i64, b: i64) -> i64 {
        self.randrange2(a, b + 1)
    }

    /// `uniform(a, b)`
    pub fn uniform(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.random()
    }

    /// Index chosen by `choice(seq)` for a sequence of length `len`.
    pub fn choice_index(&mut self, len: usize) -> usize {
        assert!(len > 0, "Cannot choose from an empty sequence");
        self.randbelow(len as u64) as usize
    }

    /// `choice(seq)`
    pub fn choice<'a, T>(&mut self, seq: &'a [T]) -> &'a T {
        &seq[self.choice_index(seq.len())]
    }

    /// `shuffle(x)`
    pub fn shuffle<T>(&mut self, x: &mut [T]) {
        for i in (1..x.len()).rev() {
            let j = self.randbelow((i + 1) as u64) as usize;
            x.swap(i, j);
        }
    }

    /// Indices picked by `sample(population, k)` for a population of length `n`.
    pub fn sample_indices(&mut self, n: usize, k: usize) -> Vec<usize> {
        assert!(k <= n, "Sample larger than population or is negative");
        let mut result = Vec::with_capacity(k);
        let mut setsize = 21usize;
        if k > 5 {
            let exp = ((k * 3) as f64).ln() / 4f64.ln();
            setsize += 4usize.pow(exp.ceil() as u32);
        }
        if n <= setsize {
            let mut pool: Vec<usize> = (0..n).collect();
            for i in 0..k {
                let j = self.randbelow((n - i) as u64) as usize;
                result.push(pool[j]);
                pool[j] = pool[n - i - 1];
            }
        } else {
            let mut selected = std::collections::HashSet::with_capacity(k);
            for _ in 0..k {
                let mut j = self.randbelow(n as u64) as usize;
                while selected.contains(&j) {
                    j = self.randbelow(n as u64) as usize;
                }
                selected.insert(j);
                result.push(j);
            }
        }
        result
    }

    /// `sample(population, k)`
    pub fn sample<T: Clone>(&mut self, population: &[T], k: usize) -> Vec<T> {
        self.sample_indices(population.len(), k).into_iter().map(|i| population[i].clone()).collect()
    }

    /// `choices(population, weights, k=1)` returning one index.
    pub fn weighted_index(&mut self, weights: &[f64]) -> usize {
        let n = weights.len();
        assert!(n > 0, "Cannot choose from an empty population");
        let mut cum = Vec::with_capacity(n);
        let mut total = 0.0;
        for w in weights {
            total += *w;
            cum.push(total);
        }
        assert!(total > 0.0 && total.is_finite(), "Total of weights must be greater than zero");
        let x = self.random() * total;
        // bisect_right(cum, x, 0, n - 1)
        let hi = n - 1;
        let mut lo = 0usize;
        let mut hi_b = hi;
        while lo < hi_b {
            let mid = (lo + hi_b) / 2;
            if x < cum[mid] {
                hi_b = mid;
            } else {
                lo = mid + 1;
            }
        }
        lo
    }

    /// `choices(population, k=1)` without weights: `floor(random() * n)`.
    pub fn choices_index(&mut self, n: usize) -> usize {
        (self.random() * n as f64).floor() as usize
    }

    /// `gauss(mu, sigma)` (caches the second value like CPython).
    pub fn gauss(&mut self, mu: f64, sigma: f64) -> f64 {
        let z = match self.gauss_next.take() {
            Some(z) => z,
            None => {
                let x2pi = self.random() * TWOPI;
                let g2rad = (-2.0 * (1.0 - self.random()).ln()).sqrt();
                self.gauss_next = Some(x2pi.sin() * g2rad);
                x2pi.cos() * g2rad
            }
        };
        mu + z * sigma
    }

    /// `normalvariate(mu, sigma)`
    pub fn normalvariate(&mut self, mu: f64, sigma: f64) -> f64 {
        let magic = nv_magicconst();
        loop {
            let u1 = self.random();
            let u2 = 1.0 - self.random();
            let z = magic * (u1 - 0.5) / u2;
            let zz = z * z / 4.0;
            if zz <= -u2.ln() {
                return mu + z * sigma;
            }
        }
    }

    /// `triangular(low, high, mode)`; `mode = None` is the midpoint.
    pub fn triangular(&mut self, low: f64, high: f64, mode: Option<f64>) -> f64 {
        let mut u = self.random();
        if high == low {
            return low;
        }
        let mut c = match mode {
            None => 0.5,
            Some(m) => (m - low) / (high - low),
        };
        let (mut lo, mut hi) = (low, high);
        if u > c {
            u = 1.0 - u;
            c = 1.0 - c;
            std::mem::swap(&mut lo, &mut hi);
        }
        lo + (hi - lo) * (u * c).sqrt()
    }

    /// `expovariate(lambd)`
    pub fn expovariate(&mut self, lambd: f64) -> f64 {
        -(1.0 - self.random()).ln() / lambd
    }
}

/// Split a big-endian unsigned integer into the little-endian 32-bit key
/// CPython's `init_by_array` receives, dropping leading zero words.
fn words_from_be_bytes(bytes: &[u8]) -> Vec<u32> {
    let mut words = Vec::with_capacity(bytes.len() / 4 + 1);
    let mut end = bytes.len();
    while end > 0 {
        let start = end.saturating_sub(4);
        let mut w = 0u32;
        for b in &bytes[start..end] {
            w = (w << 8) | *b as u32;
        }
        words.push(w);
        end = start;
    }
    while words.len() > 1 && *words.last().unwrap() == 0 {
        words.pop();
    }
    if words.is_empty() {
        words.push(0);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reference values produced by CPython 3.13:
    //   r = random.Random(<seed>); [r.random() for _ in range(3)]
    #[test]
    fn int_seed_matches_cpython() {
        let mut r = PyRandom::from_int(42);
        assert_eq!(r.random(), 0.6394267984578837);
        assert_eq!(r.random(), 0.025010755222666936);
        assert_eq!(r.random(), 0.27502931836911926);
    }

    #[test]
    fn zero_seed_matches_cpython() {
        let mut r = PyRandom::from_int(0);
        assert_eq!(r.random(), 0.8444218515250481);
    }
}
