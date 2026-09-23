//! Async resolved-value ownership without an executor or additional dependencies.
#![forbid(unsafe_code)]

#[cfg(all(test, not(miri)))]
mod support;

use std::{
    future::Future,
    pin::pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Waker},
};
use systasis::container::Error;

fn pending(future: impl Future<Output = ()>, inspect_suspended: impl FnOnce()) {
    let mut future = pin!(future);
    assert!(matches!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Pending
    ));
    inspect_suspended();
    // Dropping the pinned owner cancels the future and drops its owned values.
}

fn is_send<T: Send>(_: &T) {}

struct Value(Arc<AtomicUsize>);
impl Drop for Value {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

macro_rules! scenario {
    ($module:ident, ($($policy:tt)*), $assert_borrowing_future:expr) => {
        mod $module {
            use super::*;
            trait IValue {}
            impl IValue for Value {}
            trait IText {}
            impl IText for String {}

            #[systasis::container($($policy)*)]
            #[test]
            fn resolved_values_are_send_and_cancellation_does_not_restore_them() {
                let drops: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
                let Ok(container) = systasis::systasis_container! {
                    register_value!(Value(drops.clone()): Value as IValue);
                    register_value!(String::from("value"): String as IText);
                }.build();
                is_send(&container);
                let value = container.try_resolve_i_value().unwrap();
                let mut text = container.resolve_clone_i_text();
                let future = async move {
                    std::future::pending::<()>().await;
                    text.push('!');
                    std::hint::black_box((value, text));
                };
                is_send(&future);
                pending(future, || {
                    assert_eq!(drops.load(Ordering::SeqCst), 0);
                    assert!(matches!(container.try_resolve_i_value(), Err(Error::ValueAlreadyConsumed)));
                    assert_eq!(container.resolve_clone_i_text(), "value");
                });
                assert_eq!(drops.load(Ordering::SeqCst), 1);
                assert!(matches!(container.try_resolve_i_value(), Err(Error::ValueAlreadyConsumed)));
                let Ok(text) = container.try_resolve_clone_i_text();
                assert_eq!(text, "value");
            }

            mod polling {
                use super::*;
                #[systasis::container($($policy)*)]
                #[test]
                fn resolution_inside_a_future_occurs_only_when_polled() {
                    let drops: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
                    let Ok(container) = systasis::systasis_container! {
                        register_value!(Value(drops.clone()): Value as IValue);
                    }.build();
                    let unpolled = async {
                        let value = container.try_resolve_i_value().unwrap();
                        std::future::pending::<()>().await;
                        std::hint::black_box(value);
                    };
                    ($assert_borrowing_future)(&unpolled);
                    drop(unpolled);
                    assert_eq!(drops.load(Ordering::SeqCst), 0);
                    let polled = async {
                        let value = container.try_resolve_i_value().unwrap();
                        std::future::pending::<()>().await;
                        std::hint::black_box(value);
                    };
                    ($assert_borrowing_future)(&polled);
                    pending(polled, || {
                        assert!(matches!(container.try_resolve_i_value(), Err(Error::ValueAlreadyConsumed)));
                    });
                    assert_eq!(drops.load(Ordering::SeqCst), 1);
                }
            }
        }
    };
}
scenario!(synchronized, (require(Send, Sync)), is_send);
scenario!(local, (require(Send, !Sync)), |_: &_| {});

#[cfg(not(miri))]
#[test]
fn compiler_distinguishes_local_container_and_owned_future_traits() {
    use std::{fs, path::Path, process::Command};
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let backend = if cfg!(feature = "std") {
        "std"
    } else {
        "no-std"
    };
    let target = root.join("target/async-values").join(backend);
    let mut build = Command::new(env!("CARGO"));
    build
        .current_dir(root)
        .args(["build", "--lib", "--offline", "--locked", "--target-dir"])
        .arg(&target);
    if !cfg!(feature = "std") {
        build.arg("--no-default-features");
    }
    let artifacts = support::Artifacts::build(&mut build);
    let source = target.join("async_traits.rs");
    fs::write(
        &source,
        r#"
#![forbid(unsafe_code)]
trait IValue {}
impl IValue for String {}
fn is_send<T: Send>(_: &T) {}
#[cfg(container_sync)]
fn is_sync<T: Sync>(_: &T) {}
#[systasis::container(require(Send, !Sync))]
fn main() {
    let Ok(container) = systasis::systasis_container! {
        register_value!(String::from("value"): String as IValue);
    }.build();
    is_send(&container);
    #[cfg(container_sync)]
    is_sync(&container);
    #[cfg(container_reference)]
    {
        let future = async {
            let value = container.resolve_clone_i_value();
            std::future::pending::<()>().await;
            std::hint::black_box((value, &container));
        };
        is_send(&future);
    }
    // An owned result does not retain a reference to the !Sync container.
    let value = container.resolve_clone_i_value();
    let future = async move {
        std::future::pending::<()>().await;
        std::hint::black_box(value);
    };
    is_send(&future);
    assert_eq!(container.resolve_clone_i_value(), "value");
}
"#,
    )
    .expect("write compiler fixture");
    for (case, fragments) in [
        ("baseline", &[][..]),
        (
            "container_reference",
            &["error[E0277]", "used within this `async` block", "Sync"][..],
        ),
        (
            "container_sync",
            &["cannot be shared between threads safely", "Sync"][..],
        ),
    ] {
        let mut command = artifacts.rustc();
        command
            .args(["--edition=2024", "--crate-name", case, "--cfg", case])
            .arg(&source)
            .arg("--out-dir")
            .arg(&target);
        let output = command.output().expect("compile async trait case");
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        fs::write(target.join(format!("{case}.log")), diagnostics.as_bytes())
            .expect("retain compiler diagnostics");
        if fragments.is_empty() {
            assert!(output.status.success(), "{diagnostics}");
            assert!(
                Command::new(target.join(case))
                    .status()
                    .expect("run baseline")
                    .success()
            );
        } else {
            assert!(!output.status.success(), "{case} unexpectedly compiled");
            let location = format!("--> {}:", source.display());
            let call = if case == "container_sync" {
                "is_sync(&container);"
            } else {
                "is_send(&future);"
            };
            let expected_error = diagnostics.split("\nerror").any(|block| {
                let block = format!("error{}", block.strip_prefix("error").unwrap_or(block));
                (block.starts_with("error:") || block.starts_with("error["))
                    && block.contains(&location)
                    && block.contains(call)
                    && fragments.iter().all(|fragment| block.contains(fragment))
            });
            assert!(
                expected_error,
                "{case}: missing intended diagnostic: {diagnostics}"
            );
        }
    }
}
