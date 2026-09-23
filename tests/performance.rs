//! Reproducible host baseline, not a universal overhead or timing guarantee.
//! Run: `cargo test --release --test performance --offline -- --ignored --nocapture`
//! Repeat with `--no-default-features` to select spin for synchronized storage.
#![cfg(not(miri))]
#![forbid(unsafe_code)]

use std::{
    cell::Cell,
    hint::black_box,
    process::Command,
    time::{Duration, Instant},
};
use systasis::__private::{CopySlot, LocalTakeSlot, ReadSlot, TakeSlot};

const ITERATIONS: u64 = 100_000;
const SAMPLES: usize = 9;

#[derive(Clone, Copy, Debug)]
enum Work {
    Copy,
    Clone,
    Lazy,
    Consume,
    BuildDrop,
}

#[derive(Clone)]
struct Value(u64);
trait IValue {}
impl IValue for Value {}
impl IValue for u64 {}

fn timed(mut operation: impl FnMut(u64) -> u64) -> (Duration, u64) {
    let start = Instant::now();
    let checksum = (0..ITERATIONS).fold(0u64, |checksum, index| {
        checksum.wrapping_add(black_box(operation(black_box(index))))
    });
    (start.elapsed(), black_box(checksum))
}

mod copying {
    use super::*;
    #[systasis::container]
    pub fn generated() -> (Duration, u64) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(black_box(17u64): u64 as IValue);
        }
        .build();
        timed(|_| black_box(&container).resolve_i_value())
    }
    pub fn handwritten() -> (Duration, u64) {
        let slot = CopySlot::new(black_box(17u64));
        timed(|_| black_box(&slot).resolve())
    }
}

macro_rules! cloning {
    ($module:ident, $requirements:tt) => {
        mod $module {
            use super::*;
            #[systasis::container $requirements]
            pub fn generated() -> (Duration, u64) {
                let Ok(container) = systasis::systasis_container! {
                    register_value!(Value(black_box(0)): Value as IValue);
                }.build();
                timed(|_| black_box(&container).resolve_i_value_clone().0)
            }
            pub fn handwritten() -> (Duration, u64) {
                let slot = ReadSlot::new(Value(black_box(0)));
                timed(|_| black_box(&slot).resolve_clone().0)
            }
        }
    };
}
cloning!(synchronized, ());
cloning!(local, (require(!Sync)));

mod lazy {
    use super::*;
    fn construct(calls: &Cell<u64>) -> Value {
        let next = calls.get() + 1;
        calls.set(next);
        Value(next)
    }
    #[systasis::container(require(!Sync))]
    pub fn generated() -> (Duration, u64) {
        let calls: Cell<u64> = Cell::new(0);
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(Value as IValue, || construct(&calls));
        }
        .build();
        timed(|_| black_box(&container).resolve_i_value().0)
    }
    pub fn handwritten() -> (Duration, u64) {
        let calls = Cell::new(0);
        timed(|_| construct(black_box(&calls)).0)
    }
}

struct Dropped<'a> {
    value: u64,
    drops: &'a Cell<u64>,
}
impl Drop for Dropped<'_> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
impl IValue for Dropped<'_> {}

macro_rules! lifecycle {
    ($module:ident, $requirements:tt, $slot:ident) => {
        mod $module {
            use super::*;
            #[systasis::container $requirements]
            #[inline(never)]
            fn generated_once(index: u64, drops: &Cell<u64>, consume: bool) -> u64 {
                let Ok(container) = systasis::systasis_container! {
                    register_value!(Dropped { value: index, drops }: Dropped<'_> as IValue);
                }.build();
                if consume {
                    let value = black_box(&container).try_resolve_i_value().unwrap();
                    black_box(value.value)
                } else {
                    black_box(container);
                    index
                }
            }
            #[inline(never)]
            fn handwritten_once(index: u64, drops: &Cell<u64>, consume: bool) -> u64 {
                let owner = $slot::new(Dropped { value: index, drops });
                let slot = &owner;
                if consume {
                    let value = black_box(slot).try_resolve().unwrap();
                    black_box(value.value)
                } else {
                    black_box(slot);
                    index
                }
            }
            pub fn generated(consume: bool) -> (Duration, u64) {
                let drops = Cell::new(0);
                let result = timed(|index| generated_once(index, &drops, consume));
                assert_eq!(drops.get(), ITERATIONS);
                result
            }
            pub fn handwritten(consume: bool) -> (Duration, u64) {
                let drops = Cell::new(0);
                let result = timed(|index| handwritten_once(index, &drops, consume));
                assert_eq!(drops.get(), ITERATIONS);
                result
            }
        }
    };
}
lifecycle!(synchronized_lifecycle, (), TakeSlot);
lifecycle!(local_lifecycle, (require(!Sync)), LocalTakeSlot);

fn expected(work: Work) -> u64 {
    match work {
        Work::Copy => 17 * ITERATIONS,
        Work::Clone => 0,
        Work::Lazy => ITERATIONS * (ITERATIONS + 1) / 2,
        Work::Consume | Work::BuildDrop => ITERATIONS * (ITERATIONS - 1) / 2,
    }
}

fn report(
    name: &str,
    work: Work,
    generated: impl Fn() -> (Duration, u64),
    handwritten: impl Fn() -> (Duration, u64),
) {
    // Warm each path once, then alternate first-run order to reduce systematic
    // ordering bias. Correctness assertions are outside every timed region.
    assert_eq!(generated().1, expected(work));
    assert_eq!(handwritten().1, expected(work));
    let mut generated_samples = Vec::with_capacity(SAMPLES);
    let mut handwritten_samples = Vec::with_capacity(SAMPLES);
    for sample in 0..SAMPLES {
        let (left, right) = if sample % 2 == 0 {
            (generated(), handwritten())
        } else {
            let right = handwritten();
            (generated(), right)
        };
        assert_eq!(left.1, expected(work));
        assert_eq!(right.1, expected(work));
        generated_samples.push(left.0.as_secs_f64() * 1e9 / ITERATIONS as f64);
        handwritten_samples.push(right.0.as_secs_f64() * 1e9 / ITERATIONS as f64);
    }
    generated_samples.sort_by(f64::total_cmp);
    handwritten_samples.sort_by(f64::total_cmp);
    eprintln!(
        "{name}: generated ns/op min={:.3} median={:.3} max={:.3}; handwritten min={:.3} median={:.3} max={:.3}",
        generated_samples[0],
        generated_samples[SAMPLES / 2],
        generated_samples[SAMPLES - 1],
        handwritten_samples[0],
        handwritten_samples[SAMPLES / 2],
        handwritten_samples[SAMPLES - 1]
    );
}

#[test]
#[ignore = "release-only host timing baseline; reports samples without speed thresholds"]
fn generated_and_handwritten_runtime_baselines() {
    if cfg!(debug_assertions) {
        panic!(
            "run this ignored baseline with cargo test --release --test performance -- --ignored --nocapture"
        );
    }
    let rustc = Command::new("rustc")
        .arg("--version")
        .arg("--verbose")
        .output()
        .expect("read compiler identity");
    assert!(rustc.status.success());
    eprintln!("{}", String::from_utf8_lossy(&rustc.stdout));
    eprintln!(
        "execution={} / {}, std={}, unchecked={}, portable-atomic={}, experimental-hardware={}, iterations={ITERATIONS}, samples={SAMPLES}, profile=release",
        std::env::consts::ARCH,
        std::env::consts::OS,
        cfg!(feature = "std"),
        cfg!(feature = "resolve_unchecked"),
        cfg!(feature = "portable-atomic"),
        cfg!(feature = "experimental-hardware")
    );
    for variable in ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"] {
        eprintln!("{variable}={:?}", std::env::var_os(variable));
    }
    eprintln!(
        "Copy/clone timings exclude build/drop. Consume includes build+checked take+drop; build/drop includes one payload destruction. All paths use identical slot primitives; no allocator workload, compile-time, machine isolation, or cross-machine claims."
    );
    report("copy", Work::Copy, copying::generated, copying::handwritten);
    report(
        "clone",
        Work::Clone,
        synchronized::generated,
        synchronized::handwritten,
    );
    report(
        "local clone",
        Work::Clone,
        local::generated,
        local::handwritten,
    );

    report(
        "lazy constructor",
        Work::Lazy,
        lazy::generated,
        lazy::handwritten,
    );
    for work in [Work::Consume, Work::BuildDrop] {
        let consume = matches!(work, Work::Consume);
        report(
            &format!("synchronized {work:?}"),
            work,
            || synchronized_lifecycle::generated(consume),
            || synchronized_lifecycle::handwritten(consume),
        );
        report(
            &format!("local {work:?}"),
            work,
            || local_lifecycle::generated(consume),
            || local_lifecycle::handwritten(consume),
        );
    }
}
