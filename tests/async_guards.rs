//! Async guard ownership without an executor or additional dependencies.
#![forbid(unsafe_code)]

use std::{
    future::Future,
    pin::pin,
    task::{Context, Poll, Waker},
};
use systasis::app_container::Error;

fn pending(future: impl Future<Output = ()>) {
    let mut future = pin!(future);
    assert!(matches!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Pending
    ));
    // Dropping the pinned owner cancels the future, including its held guard.
}

fn is_send<T: Send>(_: &T) {}
fn is_sync<T: Sync>(_: &T) {}

macro_rules! scenario {
    ($module:ident, ($($policy:tt)*), $assert_future:expr) => {
        mod $module {
            use super::*;
            trait IValue {}
            impl IValue for String {}
            #[systasis::container($($policy)*)]
            #[test]
            fn cancellation_releases_shared_and_exclusive_guards() -> Result<(), Error> {
                let Ok(container) = systasis::systasis_container! {
                    register_value!(String::from("value"): String as IValue);
                }.build();
                is_send(container);
                let reader = container.try_resolve_i_value_ref()?;
                let read_future = async move {
                    std::future::pending::<()>().await;
                    std::hint::black_box(&*reader);
                };
                ($assert_future)(&read_future);
                assert!(matches!(container.try_resolve_i_value_ref_mut(), Err(Error::ValueAccessContention)));
                pending(read_future);
                let mut writer = container.try_resolve_i_value_ref_mut()?;
                writer.push('!');
                let write_future = async move {
                    std::future::pending::<()>().await;
                    std::hint::black_box(&mut *writer);
                };
                ($assert_future)(&write_future);
                assert!(matches!(container.try_resolve_i_value_ref(), Err(Error::ValueAccessContention)));
                pending(write_future);
                assert_eq!(container.try_resolve_i_value()?, "value!");
                Ok(())
            }
        }
    };
}
scenario!(synchronized, (require(Send, Sync)), is_send);
scenario!(local, (require(Send, !Sync)), |_: &_| {});

mod released {
    use super::*;
    trait IValue {}
    impl IValue for String {}

    #[systasis::container(require(Send, Sync))]
    #[test]
    fn synchronized_container_reference_can_remain_after_guard_release() {
        let Ok(container) = systasis::systasis_container! {
            register_value!(String::from("value"): String as IValue);
        }
        .build();
        is_send(container);
        is_sync(container);
        let future = async {
            {
                let mut writer = container.try_resolve_i_value_ref_mut().unwrap();
                writer.push('!');
            }
            std::future::pending::<()>().await;
            std::hint::black_box(container);
        };
        is_send(&future);
        pending(future);
        assert_eq!(container.try_resolve_i_value().unwrap(), "value!");
    }
}

#[cfg(not(miri))]
#[test]
fn compiler_distinguishes_local_container_and_future_traits() {
    use std::{fs, path::Path, process::Command};
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let backend = if cfg!(feature = "std") {
        "std"
    } else {
        "no-std"
    };
    let target = root.join("target/async-guards").join(backend);
    let mut build = Command::new(env!("CARGO"));
    build
        .current_dir(root)
        .args(["build", "--lib", "--offline", "--locked", "--target-dir"])
        .arg(&target);
    if !cfg!(feature = "std") {
        build.arg("--no-default-features");
    }
    let output = build.output().expect("build compiler fixture dependency");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
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
    is_send(container);
    #[cfg(container_sync)]
    is_sync(container);
    #[cfg(container_reference)]
    {
        let future = async {
            { let guard = container.try_resolve_i_value_ref().unwrap(); std::hint::black_box(&*guard); }
            std::future::pending::<()>().await;
            std::hint::black_box(container);
        };
        is_send(&future);
    }
    #[cfg(any(held_read, held_write))]
    {
        #[cfg(held_read)]
        let guard = container.try_resolve_i_value_ref().unwrap();
        #[cfg(held_write)]
        let guard = container.try_resolve_i_value_ref_mut().unwrap();
        let future = async move {
            std::future::pending::<()>().await;
            std::hint::black_box(&*guard);
        };
        is_send(&future);
    }
    // Neither the local guard nor &!Sync container enters this future.
    let value = { container.try_resolve_i_value_ref().unwrap().clone() };
    let future = async move {
        std::future::pending::<()>().await;
        std::hint::black_box(value);
    };
    is_send(&future);
    assert!(container.try_resolve_i_value_ref_mut().is_ok());
}
"#,
    )
    .expect("write compiler fixture");
    for (case, fragments) in [
        ("baseline", &[][..]),
        (
            "held_read",
            &[
                "future cannot be sent between threads safely",
                "Send",
                "cell::Ref<'_, String>",
            ][..],
        ),
        (
            "held_write",
            &[
                "future cannot be sent between threads safely",
                "Send",
                "cell::RefMut<'_, String>",
            ][..],
        ),
        (
            "container_reference",
            &[
                "error[E0277]",
                "used within this `async` block",
                "Sync",
                "RefCell",
            ][..],
        ),
        (
            "container_sync",
            &["cannot be shared between threads safely", "Sync", "RefCell"][..],
        ),
    ] {
        let mut command = Command::new("rustc");
        command
            .args(["--edition=2024", "--crate-name", case, "--cfg", case])
            .arg(&source)
            .arg("--out-dir")
            .arg(&target)
            .arg("--extern")
            .arg(format!(
                "systasis={}",
                target.join("debug/libsystasis.rlib").display()
            ))
            .arg("-L")
            .arg(format!(
                "dependency={}",
                target.join("debug/deps").display()
            ));
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
            assert!(diagnostics.contains("error"), "{diagnostics}");
            for fragment in fragments {
                assert!(
                    diagnostics.contains(fragment),
                    "{case}: missing {fragment}: {diagnostics}"
                );
            }
        }
    }
}
