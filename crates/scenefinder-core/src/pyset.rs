//! Reproduces the iteration order of a CPython `set` of floats.
//!
//! The reference scene finder collects the candidate window starts of a
//! round-winning scene in a Python set and keeps the first best-scoring one it
//! iterates over, so which of several equal windows is reported depends on
//! CPython's hash table layout. This module mirrors CPython 3.12's float hash
//! and set insertion (open addressing, 9 linear probes, perturbation, resize
//! at 60% fill) closely enough to iterate in the same order.

const HASH_BITS: u32 = 61;
const HASH_MODULUS: u64 = (1 << HASH_BITS) - 1;
const HASH_INF: i64 = 314159;
const LINEAR_PROBES: usize = 9;
const PERTURB_SHIFT: u32 = 5;
const MIN_SIZE: usize = 8;

fn frexp(v: f64) -> (f64, i32) {
    if v == 0.0 || !v.is_finite() {
        return (v, 0);
    }
    let bits = v.to_bits();
    let exp = ((bits >> 52) & 0x7ff) as i32;
    if exp == 0 {
        let (m, e) = frexp(v * f64::from_bits((1023 + 54) << 52));
        return (m, e - 54);
    }
    let m = f64::from_bits((bits & !(0x7ffu64 << 52)) | (1022u64 << 52));
    (m, exp - 1022)
}

/// CPython's `hash()` of a finite float.
pub fn py_hash_f64(v: f64) -> i64 {
    if v.is_infinite() {
        return if v > 0.0 { HASH_INF } else { -HASH_INF };
    }
    if v.is_nan() {
        return 0;
    }
    let (mut m, mut e) = frexp(v);
    let mut sign: i64 = 1;
    if m < 0.0 {
        sign = -1;
        m = -m;
    }
    let mut x: u64 = 0;
    while m != 0.0 {
        x = ((x << 28) & HASH_MODULUS) | (x >> (HASH_BITS - 28));
        m *= 268435456.0;
        e -= 28;
        let y = m as u64;
        m -= y as f64;
        x += y;
        if x >= HASH_MODULUS {
            x -= HASH_MODULUS;
        }
    }
    let e = if e >= 0 {
        e % HASH_BITS as i32
    } else {
        HASH_BITS as i32 - 1 - ((-1 - e) % HASH_BITS as i32)
    } as u32;
    x = ((x << e) & HASH_MODULUS) | (x >> (HASH_BITS - e));
    let mut h = (x as i64).wrapping_mul(sign);
    if h == -1 {
        h = -2;
    }
    h
}

pub struct PySet {
    table: Vec<Option<(i64, f64)>>,
    mask: usize,
    used: usize,
    fill: usize,
}

impl Default for PySet {
    fn default() -> Self {
        Self::new()
    }
}

impl PySet {
    pub fn new() -> Self {
        PySet {
            table: vec![None; MIN_SIZE],
            mask: MIN_SIZE - 1,
            used: 0,
            fill: 0,
        }
    }

    pub fn add(&mut self, key: f64) {
        let hash = py_hash_f64(key);
        let mask = self.mask;
        let mut i = (hash as u64 as usize) & mask;
        let mut perturb = hash as u64;
        let slot = loop {
            let mut idx = i;
            let mut probes = if i + LINEAR_PROBES <= mask {
                LINEAR_PROBES
            } else {
                0
            };
            let found = loop {
                match self.table[idx] {
                    None => break Some(idx),
                    Some((h, k)) if h == hash && k == key => return,
                    Some(_) => {}
                }
                if probes == 0 {
                    break None;
                }
                probes -= 1;
                idx += 1;
            };
            if let Some(idx) = found {
                break idx;
            }
            perturb >>= PERTURB_SHIFT;
            i = ((i as u64)
                .wrapping_mul(5)
                .wrapping_add(1)
                .wrapping_add(perturb) as usize)
                & mask;
        };
        self.table[slot] = Some((hash, key));
        self.fill += 1;
        self.used += 1;
        if self.fill * 5 >= mask * 3 {
            let minused = if self.used > 50000 {
                self.used * 2
            } else {
                self.used * 4
            };
            self.resize(minused);
        }
    }

    fn resize(&mut self, minused: usize) {
        let mut newsize = MIN_SIZE;
        while newsize <= minused {
            newsize <<= 1;
        }
        let mask = newsize - 1;
        let mut table: Vec<Option<(i64, f64)>> = vec![None; newsize];
        for entry in self.table.iter().flatten() {
            let (hash, _) = *entry;
            let mut i = (hash as u64 as usize) & mask;
            let mut perturb = hash as u64;
            let slot = 'find: loop {
                if table[i].is_none() {
                    break i;
                }
                if i + LINEAR_PROBES <= mask {
                    for j in 1..=LINEAR_PROBES {
                        if table[i + j].is_none() {
                            break 'find i + j;
                        }
                    }
                }
                perturb >>= PERTURB_SHIFT;
                i = ((i as u64)
                    .wrapping_mul(5)
                    .wrapping_add(1)
                    .wrapping_add(perturb) as usize)
                    & mask;
            };
            table[slot] = Some(*entry);
        }
        self.table = table;
        self.mask = mask;
        self.fill = self.used;
    }

    /// Keys in CPython's iteration order.
    pub fn iter(&self) -> impl Iterator<Item = f64> + '_ {
        self.table.iter().flatten().map(|(_, k)| *k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_hash_matches_cpython() {
        assert_eq!(py_hash_f64(0.5), 1152921504606846976);
        assert_eq!(py_hash_f64(1.5), 1152921504606846977);
        assert_eq!(py_hash_f64(2.25), 576460752303423490);
        assert_eq!(py_hash_f64(3.0), 3);
        assert_eq!(py_hash_f64(0.0), 0);
        assert_eq!(py_hash_f64(-1.0), -2);
        assert_eq!(py_hash_f64(f64::from(0.1f32)), 230584304357343232);
        assert_eq!(py_hash_f64(531.6777), 1562669807344026131);
        assert_eq!(py_hash_f64(531.7162), 1651444763198751251);
        assert_eq!(py_hash_f64(f64::from(531.7f32)), 1614118253947257363);
    }

    fn order(items: &[f64]) -> Vec<f64> {
        let mut s = PySet::new();
        for &x in items {
            s.add(x);
        }
        s.iter().collect()
    }

    #[test]
    fn set_order_matches_cpython() {
        assert_eq!(order(&[531.6777, 531.7162]), vec![531.7162, 531.6777]);
        let f = |x: f32| f64::from(x);
        assert_eq!(
            order(&[1.5, 2.25, 3.0, f(0.1), f(100.7), f(531.7)]),
            vec![f(0.1), 1.5, 2.25, 3.0, f(100.7), f(531.7)]
        );
        assert_eq!(order(&[0.5, 5.0, 0.0, -1.0]), vec![0.5, 0.0, 5.0, -1.0]);
        assert_eq!(
            order(&[10.0, 11.0, 12.0, 9.0, 10.0]),
            vec![9.0, 10.0, 11.0, 12.0]
        );
        let items: Vec<f64> = (0..40)
            .map(|i| f64::from((f64::from(i) * 0.37) as f32))
            .collect();
        let expect_idx = [
            0, 1, 4, 2, 3, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 17, 19, 22, 25, 28, 30, 33, 36, 38,
            16, 15, 18, 20, 21, 23, 24, 26, 27, 29, 31, 32, 34, 35, 37, 39,
        ];
        let expect: Vec<f64> = expect_idx.iter().map(|&i| items[i]).collect();
        assert_eq!(order(&items), expect);
    }
}
