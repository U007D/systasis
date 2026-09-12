//! Native closures retain exact owned tails of generic array aliases.
#![forbid(unsafe_code)]

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

type Input<T> = [T; 3];
trait ILength {}
impl ILength for usize {}

struct Tracked(Arc<AtomicUsize>);

impl Drop for Tracked {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[systasis::container(require(Send, Sync))]
fn run<T: Send + Sync>(input: Input<T>) {
    let [head, tail @ ..]: Input<T> = input;
    let Ok(container) = systasis::systasis_container! {
        register_type_with!(usize as ILength, || {
            let exact: &[T; 2] = &tail;
            // The macro also selects native capture storage for this case.
            assert_eq!(exact.len(), 2);
            exact.len()
        });
    }
    .build();
    // Capturing the tail leaves the unrelated head owned by this scope.
    drop(head);
    assert_eq!(container.resolve_i_length(), 2);
    assert_eq!(container.resolve_i_length(), 2);
}

#[test]
fn owned_generic_array_tail_keeps_exact_type_and_drops_once() {
    let drops: [Arc<AtomicUsize>; 3] = std::array::from_fn(|_| Arc::new(AtomicUsize::new(0)));
    run(std::array::from_fn(|index| Tracked(drops[index].clone())));
    assert_eq!(drops.map(|count| count.load(Ordering::SeqCst)), [1, 1, 1]);
}
