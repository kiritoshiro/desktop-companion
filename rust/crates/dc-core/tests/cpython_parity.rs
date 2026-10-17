//! Replays draws recorded from CPython 3.13 (`tests/fixtures/cpython_random.json`).

use dc_core::PyRandom;
use serde_json::Value;

fn rng_for(case: &Value) -> PyRandom {
    let seed = case["seed"].as_str().unwrap();
    match case["kind"].as_str().unwrap() {
        "str" => PyRandom::from_str_seed(seed),
        _ => PyRandom::from_int(seed.parse::<i128>().unwrap()),
    }
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

#[test]
fn every_recorded_draw_matches_cpython() {
    let data: Vec<Value> = serde_json::from_str(include_str!("fixtures/cpython_random.json")).unwrap();
    for case in &data {
        let seed = case["seed"].clone();
        let mut r = rng_for(case);
        for v in case["random"].as_array().unwrap() {
            assert_eq!(r.random(), f(v), "random() seed {seed}");
        }
        for pair in case["getrandbits"].as_array().unwrap() {
            let k = pair[0].as_u64().unwrap() as u32;
            assert_eq!(r.getrandbits(k), pair[1].as_u64().unwrap(), "getrandbits({k}) seed {seed}");
        }
        for (n, v) in [1i64, 2, 3, 10, 1000, (1 << 40) + 3].iter().zip(case["randrange"].as_array().unwrap()) {
            assert_eq!(r.randrange(*n), v.as_i64().unwrap(), "randrange({n}) seed {seed}");
        }
        for v in case["randint"].as_array().unwrap() {
            assert_eq!(r.randint(-5, 5), v.as_i64().unwrap());
        }
        for v in case["uniform"].as_array().unwrap() {
            assert_eq!(r.uniform(-3.0, 9.5), f(v));
        }
        for (n, v) in [1usize, 5, 17].iter().zip(case["choice"].as_array().unwrap()) {
            let seq: Vec<usize> = (0..*n).collect();
            assert_eq!(*r.choice(&seq) as u64, v.as_u64().unwrap());
        }
        let mut x: Vec<u64> = (0..12).collect();
        r.shuffle(&mut x);
        let want: Vec<u64> = case["shuffle"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
        assert_eq!(x, want, "shuffle seed {seed}");
        let got: Vec<u64> = r.sample_indices(30, 6).into_iter().map(|i| i as u64).collect();
        let want: Vec<u64> = case["sample_small"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
        assert_eq!(got, want, "sample small seed {seed}");
        let got: Vec<u64> = r.sample_indices(5000, 7).into_iter().map(|i| i as u64).collect();
        let want: Vec<u64> = case["sample_big"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
        assert_eq!(got, want, "sample big seed {seed}");
        for v in case["gauss"].as_array().unwrap() {
            assert!((r.gauss(1.0, 2.0) - f(v)).abs() < 1e-12, "gauss seed {seed}");
        }
        for v in case["normalvariate"].as_array().unwrap() {
            assert!((r.normalvariate(0.0, 1.0) - f(v)).abs() < 1e-12);
        }
        let t = case["triangular"].as_array().unwrap();
        assert!((r.triangular(0.0, 10.0, Some(3.0)) - f(&t[0])).abs() < 1e-12);
        assert!((r.triangular(2.0, 4.0, None) - f(&t[1])).abs() < 1e-12);
        for v in case["weighted"].as_array().unwrap() {
            assert_eq!(r.weighted_index(&[1.0, 0.5, 3.0, 2.0]) as u64, v.as_u64().unwrap());
        }
        for v in case["choices"].as_array().unwrap() {
            assert_eq!(r.choices_index(7) as u64, v.as_u64().unwrap());
        }
        assert!((r.expovariate(1.5) - f(&case["expovariate"][0])).abs() < 1e-12);
    }
}
