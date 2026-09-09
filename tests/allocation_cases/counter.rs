//! Test-only allocation instrumentation; never linked into the library.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct Counts {
    pub allocations: usize,
    pub zeroed_allocations: usize,
    pub reallocations: usize,
    pub deallocations: usize,
}

thread_local! {
    // No destructor, formatting, locking, or allocator recursion in this state.
    static ACTIVE: Cell<Option<Counts>> = const { Cell::new(None) };
}

enum Event {
    Allocate,
    Zeroed,
    Reallocate,
    Deallocate,
}

fn record(event: Event) {
    // Thread-local access does not use #[global_allocator]. During TLS teardown,
    // try_with can fail; those calls are outside any live measurement window.
    // https://doc.rust-lang.org/std/alloc/trait.GlobalAlloc.html#re-entrance
    let _ = ACTIVE.try_with(|state| {
        if let Some(mut counts) = state.get() {
            let count = match event {
                Event::Allocate => &mut counts.allocations,
                Event::Zeroed => &mut counts.zeroed_allocations,
                Event::Reallocate => &mut counts.reallocations,
                Event::Deallocate => &mut counts.deallocations,
            };
            // Saturation preserves nonzero evidence and cannot panic on overflow.
            *count = count.saturating_add(1);
            state.set(Some(counts));
        }
    });
}

struct Counter;

#[global_allocator]
static ALLOCATOR: Counter = Counter;

// SAFETY: System handles every allocation with the caller's original arguments.
// No pointers, layouts, return values, or ownership rules are changed. Recording
// uses only non-panicking Cell/counter operations and cannot recursively allocate.
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(Event::Allocate);
        // SAFETY: the GlobalAlloc caller supplies System's identical preconditions.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(Event::Zeroed);
        // SAFETY: the nonzero layout and zero-initialization contract are unchanged.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record(Event::Reallocate);
        // SAFETY: this pointer came from System via this wrapper; the caller
        // provides its current layout and a valid nonzero new size. Forwarding
        // preserves System's ownership transfer on success and retention on failure.
        unsafe { System.realloc(pointer, layout, new_size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        record(Event::Deallocate);
        // SAFETY: all wrapper allocations come from System; pointer and matching
        // allocation layout are forwarded once, without reading freed storage.
        unsafe { System.dealloc(pointer, layout) }
    }
}

struct Window;

impl Drop for Window {
    fn drop(&mut self) {
        let _ = ACTIVE.try_with(|state| state.set(None));
    }
}

/// Count allocator calls on this thread during `operation`, including its drops.
/// Returned values are dropped by the caller, outside this measurement window.
pub(super) fn measure<R>(operation: impl FnOnce() -> R) -> (R, Counts) {
    ACTIVE.with(|state| {
        assert!(
            state.get().is_none(),
            "allocation measurement windows must not nest"
        );
        state.set(Some(Counts::default()));
    });
    let window = Window;
    let result = operation();
    let counts = ACTIVE.with(|state| {
        state.get().unwrap_or_else(|| {
            unreachable!(
                "measure initialized this thread's state; only its still-live Window clears it"
            )
        })
    });
    drop(window);
    (result, counts)
}

pub(super) fn assert_no_allocations(counts: Counts) {
    // Assertions/diagnostics run only after the measurement is disabled.
    assert_eq!(counts.allocations, 0, "{counts:?}");
    assert_eq!(counts.zeroed_allocations, 0, "{counts:?}");
    assert_eq!(counts.reallocations, 0, "{counts:?}");
}
