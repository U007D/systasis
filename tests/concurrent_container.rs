//! Generated resolvers copy and clone repeatedly, but transfer move-only values once.
#![forbid(unsafe_code)]

use std::{sync::Barrier, thread};
use systasis::container::Error;

#[derive(Debug, PartialEq, Eq)]
struct Database<'a> {
    name: &'a str,
    writes: usize,
}
trait IDatabase {}
impl IDatabase for Database<'_> {}
trait ILimit {}
impl ILimit for usize {}
trait ILabel {}
impl ILabel for String {}

#[systasis::container(require(Send, Sync))]
#[test]
fn scoped_threads_share_exactly_once_consumption_and_repeatable_values() {
    let name: String = String::from("borrowed database name");
    let Ok(container) = systasis::systasis_container! {
        register_value!(Database { name: &name, writes: 0 }: Database<'_> as IDatabase);
        register_value!(7_usize: usize as ILimit);
        register_value!(String::from("label"): String as ILabel);
    }
    .build();

    let start = Barrier::new(8);
    let outcomes = thread::scope(|scope| {
        let workers: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    start.wait();
                    let outcome = container.try_resolve_i_database();
                    for _ in 0..16 {
                        let Ok(limit) = container.try_resolve_i_limit();
                        assert_eq!(limit, 7);
                        let Ok(mut label) = container.try_resolve_i_label_clone();
                        label.push('!');
                        assert_eq!(label, "label!");
                        assert_eq!(container.resolve_i_label_clone(), "label");
                    }
                    outcome
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("resolver worker must succeed"))
            .collect::<Vec<_>>()
    });
    let mut values = outcomes.into_iter().filter_map(Result::ok);
    let mut taken = values.next().expect("one worker must acquire the value");
    assert!(values.next().is_none(), "a value must never transfer twice");
    assert_eq!(taken.name, name);
    taken.writes += 1;
    assert_eq!(taken.writes, 1);
    assert!(matches!(
        container.try_resolve_i_database(),
        Err(Error::ValueAlreadyConsumed)
    ));
    assert_eq!(container.resolve_i_limit(), 7);
}
