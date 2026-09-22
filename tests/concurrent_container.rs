//! R04/R07: generated resolvers share checked slot state across scoped threads.
#![forbid(unsafe_code)]

use std::{sync::mpsc, thread, time::Duration};
use systasis::app_container::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Database<'a> {
    name: &'a str,
    writes: usize,
}
trait IDatabase {}
impl IDatabase for Database<'_> {}
trait ILimit {}
impl ILimit for usize {}

// Hold the guard on this thread while the worker attempts incompatible access.
// A blocking regression must fail the test, not deadlock the test process: release
// the guard before unwinding or joining after the watchdog expires. The timeout
// is a test watchdog, not a promised resolver latency or a scheduling assumption.
fn while_held<G>(guard: G, check: impl FnOnce() + Send) {
    thread::scope(|scope| {
        let (done, completion) = mpsc::sync_channel(1);
        let worker = scope.spawn(move || {
            check();
            let _ = done.send(());
        });
        let completed = completion.recv_timeout(Duration::from_secs(10));
        drop(guard);
        worker.join().expect("resolver checks must succeed");
        completed.expect("resolver calls must finish while the conflicting guard is held");
    });
}

#[systasis::container(require(Send, Sync))]
#[test]
fn scoped_threads_share_contention_mutation_and_consumption_state() -> Result<(), Error> {
    let name: String = String::from("borrowed database name");
    let Ok(container) = systasis::systasis_container! {
        register_value!(Database { name: &name, writes: 0 }: Database<'_> as IDatabase);
        register_value!(7_usize: usize as ILimit);
    }
    .build();

    let reader = container.try_resolve_i_database_ref()?;
    while_held(reader, || {
        let other_reader = container.try_resolve_i_database_ref().unwrap();
        assert_eq!(other_reader.name, name);
        assert_eq!(container.try_resolve_i_database_clone().unwrap().writes, 0);
        assert!(matches!(
            container.try_resolve_i_database(),
            Err(Error::ValueAccessContention)
        ));
        assert!(matches!(
            container.try_resolve_i_database_ref_mut(),
            Err(Error::ValueAccessContention)
        ));
        assert_eq!(container.resolve_i_limit(), 7);
    });

    let writer = container.try_resolve_i_database_ref_mut()?;
    while_held(writer, || {
        assert!(matches!(
            container.try_resolve_i_database_ref(),
            Err(Error::ValueAccessContention)
        ));
        assert!(matches!(
            container.try_resolve_i_database_ref_mut(),
            Err(Error::ValueAccessContention)
        ));
        assert!(matches!(
            container.try_resolve_i_database_clone(),
            Err(Error::ValueAccessContention)
        ));
        assert!(matches!(
            container.try_resolve_i_database(),
            Err(Error::ValueAccessContention)
        ));
        assert_eq!(container.resolve_i_limit(), 7);
    });

    // A guard obtained here can be moved to another thread, used and dropped.
    let mut writer = container.try_resolve_i_database_ref_mut()?;
    thread::scope(|scope| {
        scope.spawn(move || writer.writes += 1).join().unwrap();
    });
    assert_eq!(container.try_resolve_i_database_ref()?.writes, 1);

    let taken = thread::scope(|scope| {
        scope
            .spawn(|| container.try_resolve_i_database())
            .join()
            .unwrap()
    })?;
    assert_eq!(
        taken,
        Database {
            name: &name,
            writes: 1
        }
    );
    assert!(matches!(
        container.try_resolve_i_database(),
        Err(Error::ValueAlreadyConsumed)
    ));
    assert!(matches!(
        container.try_resolve_i_database_ref(),
        Err(Error::ValueAlreadyConsumed)
    ));
    assert!(matches!(
        container.try_resolve_i_database_ref_mut(),
        Err(Error::ValueAlreadyConsumed)
    ));
    assert!(matches!(
        container.try_resolve_i_database_clone(),
        Err(Error::ValueAlreadyConsumed)
    ));
    assert_eq!(container.resolve_i_limit(), 7);
    Ok(())
}
