//! Allocator observations for specified workloads, not an optimizer-independent proof.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![cfg_attr(miri, feature(never_type))]

#[allow(unsafe_code)]
#[path = "allocation_cases/counter.rs"]
mod counter;
use counter::{Counts, assert_no_allocations, measure};

#[path = "allocation_cases/composition.rs"]
mod composition;
#[path = "allocation_cases/lifecycle.rs"]
mod lifecycle;
#[path = "allocation_cases/plain.rs"]
mod plain;

use std::hint::black_box;

#[test]
fn positive_control_observes_all_allocator_entry_points() {
    let (_, counts) = measure(|| {
        let mut bytes = Vec::with_capacity(black_box(8));
        bytes.extend_from_slice(&[1_u8; 8]);
        black_box(&bytes);
        bytes.reserve(black_box(128));
        black_box(&bytes);
        let zeros = vec![0_u8; black_box(1024)];
        black_box(&zeros);
        drop((bytes, zeros));
    });
    // Safe assertions detect an ineffective probe if optimization or a changed
    // standard-library strategy stops exercising an allocator entry point.
    assert!(counts.allocations > 0, "{counts:?}");
    assert!(counts.zeroed_allocations > 0, "{counts:?}");
    assert!(counts.reallocations > 0, "{counts:?}");
    assert!(counts.deallocations >= 2, "{counts:?}");
}

#[test]
fn empty_measurement_and_instrumentation_do_not_allocate() {
    let (_, counts) = measure(|| black_box([42_u8; 32]));
    assert_eq!(counts, Counts::default());
}

#[test]
fn unwind_disables_measurement_before_the_next_window() {
    let panic = std::panic::catch_unwind(|| measure(|| panic!("caller panic in measured code")));
    assert!(panic.is_err());
    drop(panic);
    let (_, counts) = measure(|| black_box(42));
    assert_eq!(counts, Counts::default());
}

#[test]
fn rejected_nested_window_does_not_disable_the_outer_window() {
    let (_, counts) = measure(|| {
        let nested = std::panic::catch_unwind(|| measure(|| ()));
        assert!(nested.is_err());
        drop(nested);
        let bytes = vec![1_u8; black_box(1024)];
        black_box(&bytes);
        drop(bytes);
    });
    assert!(counts.allocations > 0, "{counts:?}");
    let (_, next) = measure(|| ());
    assert_eq!(next, Counts::default());
}

#[test]
fn other_thread_allocations_do_not_pollute_the_current_window() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let start = AtomicBool::new(false);
    let finished = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            while !start.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
            let (_, counts) = measure(|| {
                let bytes = vec![1_u8; black_box(1024)];
                black_box(&bytes);
                drop(bytes);
            });
            finished.store(true, Ordering::Release);
            counts
        });
        let (_, counts) = measure(|| {
            start.store(true, Ordering::Release);
            while !finished.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
        });
        assert_eq!(counts, Counts::default());
        let other = worker.join().expect("allocation worker completed");
        assert!(other.allocations > 0, "{other:?}");
    });
}
