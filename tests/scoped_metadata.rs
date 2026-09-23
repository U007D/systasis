//! Foundation checks, not proof of generated register_container! integration.
#![forbid(unsafe_code)]

#[cfg(all(test, not(miri)))]
mod support;

use systasis::{
    __private::{CopySlot, FreshSlot, LocalTakeSlot, ReadSlot, TakeSlot},
    container::Error,
    scoped::{SlotAccess, op},
};

#[test]
fn slot_policy_preserves_copy_selection_and_adapts_owned_local_storage() {
    use systasis::{__private::Select, scoped::SlotPolicy};
    type Selected<S, T, const LOCAL: bool> = <<S as SlotPolicy<LOCAL>>::Policy as Select<T>>::Slot;
    let copied: Selected<CopySlot<u32>, u32, false> = CopySlot::new(11);
    assert_eq!(copied.resolve(), 11);
    let copied_local: Selected<CopySlot<u32>, u32, true> = CopySlot::new(12);
    assert_eq!(copied_local.resolve(), 12);
    let local: Selected<TakeSlot<u32>, u32, true> = LocalTakeSlot::new(13);
    assert_eq!(local.try_resolve().unwrap(), 13);
    let synchronized: Selected<LocalTakeSlot<u32>, u32, false> = TakeSlot::new(14);
    assert_eq!(synchronized.try_resolve().unwrap(), 14);
    let readonly: Selected<ReadSlot<u32>, u32, false> = TakeSlot::new(15);
    assert_eq!(readonly.try_resolve().unwrap(), 15);
    type Rebound =
        <systasis::__private::Policy<false, false> as systasis::scoped::RebindPolicy<true>>::Policy;
    let rebound: <Rebound as Select<u32>>::Slot = LocalTakeSlot::new(16);
    assert_eq!(rebound.try_resolve().unwrap(), 16);
}

#[test]
fn unbounded_generic_slot_policy_stays_consumable_when_instantiated_with_copy() {
    use systasis::{
        __private::Select,
        scoped::{Here, RegistrationPolicy, SlotPolicy},
    };
    struct Key;
    struct Child<T>(core::marker::PhantomData<T>);
    impl<T, const LOCAL: bool> RegistrationPolicy<Here, Key, LOCAL> for Child<T> {
        type Policy = <TakeSlot<T> as SlotPolicy<LOCAL>>::Policy;
    }
    fn transfer<T>(
        value: T,
    ) -> <<Child<T> as RegistrationPolicy<Here, Key, false>>::Policy as Select<T>>::Slot {
        <<Child<T> as RegistrationPolicy<Here, Key, false>>::Policy as Select<T>>::store(value)
    }
    let slot: TakeSlot<u32> = transfer(23);
    assert_eq!(slot.try_resolve().unwrap(), 23);
    assert!(matches!(
        slot.try_resolve(),
        Err(Error::ValueAlreadyConsumed)
    ));
}

#[test]
fn copy_dispatch_preserves_plain_references_and_repeatable_ownership() {
    let slot = CopySlot::new(17u32);
    assert_eq!(SlotAccess::<op::Owned>::access(&slot), 17);
    assert_eq!(SlotAccess::<op::Owned>::access(&slot), 17);
    assert_eq!(*SlotAccess::<op::Shared>::access(&slot), 17);
    assert_eq!(SlotAccess::<op::CloneValue>::access(&slot), 17);
}

#[test]
fn readonly_and_fresh_dispatch_preserve_their_distinct_outputs() {
    let slot = ReadSlot::new(String::from("read only"));
    let borrowed: &String = SlotAccess::<op::Shared>::access(&slot);
    assert_eq!(borrowed, "read only");
    assert_eq!(SlotAccess::<op::CloneValue>::access(&slot), "read only");
    let fresh = FreshSlot::<String>::new();
    assert!(SlotAccess::<op::Owned>::access(&fresh).is_empty());
    assert!(SlotAccess::<op::Owned>::access(&fresh).is_empty());
}

macro_rules! checked_dispatch {
    ($test:ident, $slot:ident) => {
        #[test]
        fn $test() -> Result<(), Error> {
            let slot = $slot::new(String::from("owned"));
            let guard = SlotAccess::<op::TryShared>::access(&slot)?;
            assert_eq!(&*guard, "owned");
            assert!(matches!(
                SlotAccess::<op::TryOwned>::access(&slot),
                Err(Error::ValueAccessContention)
            ));
            assert!(matches!(
                SlotAccess::<op::TryExclusive>::access(&slot),
                Err(Error::ValueAccessContention)
            ));
            drop(guard);
            SlotAccess::<op::TryExclusive>::access(&slot)?.push('!');
            assert_eq!(SlotAccess::<op::TryCloneValue>::access(&slot)?, "owned!");
            assert_eq!(SlotAccess::<op::TryOwned>::access(&slot)?, "owned!");
            assert!(matches!(
                SlotAccess::<op::TryOwned>::access(&slot),
                Err(Error::ValueAlreadyConsumed)
            ));
            assert!(matches!(
                SlotAccess::<op::TryShared>::access(&slot),
                Err(Error::ValueAlreadyConsumed)
            ));
            assert!(matches!(
                SlotAccess::<op::TryExclusive>::access(&slot),
                Err(Error::ValueAlreadyConsumed)
            ));
            assert!(matches!(
                SlotAccess::<op::TryCloneValue>::access(&slot),
                Err(Error::ValueAlreadyConsumed)
            ));
            Ok(())
        }
    };
}
checked_dispatch!(
    synchronized_dispatch_retains_guards_and_checked_failures,
    TakeSlot
);
checked_dispatch!(
    local_dispatch_retains_refcell_guards_and_checked_failures,
    LocalTakeSlot
);

#[cfg(not(miri))]
#[test]
fn cross_crate_aliases_preserve_metadata_and_restricted_guard_lifetimes() {
    use std::{fs, path::Path, process::Command};
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let backend = if cfg!(feature = "std") {
        "std"
    } else {
        "no-std"
    };
    let target = root.join("target/scoped-metadata").join(backend);
    let mut build = Command::new(env!("CARGO"));
    build
        .current_dir(root)
        .args(["build", "--lib", "--offline", "--locked", "--target-dir"])
        .arg(&target);
    if !cfg!(feature = "std") {
        build.arg("--no-default-features");
    }
    let artifacts = support::Artifacts::build(&mut build);

    let compile = |source: &Path, arguments: &[&str]| {
        artifacts
            .rustc()
            .args(["--edition=2024", "--out-dir"])
            .arg(&target)
            .arg(source)
            .args(arguments)
            .output()
            .expect("compile metadata fixture")
    };
    let provider = target.join("scope_provider.rs");
    fs::write(
        &provider,
        r#"
#![no_std]
#![forbid(unsafe_code)]
use systasis::{__private::TakeSlot, scoped::*, container::Error, Ref};
pub struct ValueKey;
pub struct NamedNamespace;
pub struct ChildKey;
pub struct Restricted;
pub struct Container<T> { slot: TakeSlot<T> }
impl<T> Container<T> { pub fn new(value: T) -> Self { Self { slot: TakeSlot::new(value) } } }
pub type Alias<T> = Container<T>;
pub mod descriptor {
    use super::*;
    pub struct Scope<'a, T> { backing: &'a Container<T> }
    impl<'a, T> Scope<'a, T> {
        pub(super) fn new(backing: &'a Container<T>) -> Self { Self { backing } }
    }
    impl<'a, T> Resolve<'a, Here, ValueKey, op::TryShared> for Scope<'a, T> {
        type Output = Result<Ref<'a, T>, Error>;
        fn resolve(&self) -> Self::Output { self.backing.slot.try_resolve_ref() }
    }
}
impl<T> Registered<Here, ValueKey> for Container<T> {
    type Value<'a> = T where Self: 'a;
}
impl<T> Registered<Here<NamedNamespace>, ValueKey> for Container<T> {
    type Value<'a> = &'a T where Self: 'a;
}
impl<T> Output<Here, ValueKey, op::TryShared> for Container<T> {
    type Value<'a> = Result<Ref<'a, T>, Error> where Self: 'a;
}
impl<T> Output<Here<NamedNamespace>, ValueKey, op::TryOwned> for Container<T> {
    type Value<'a> = Option<&'a T> where Self: 'a;
}
impl<T> AsScope<Restricted> for Container<T> {
    type Scope<'a> = descriptor::Scope<'a, T> where Self: 'a;
    fn scope(&self) -> Self::Scope<'_> { descriptor::Scope::new(self) }
}
pub type SubContainer<'a, T> = <Alias<T> as AsScope<Restricted>>::Scope<'a>;
pub struct Parent<'child, T> { child: &'child Alias<T> }
impl<T> Child<ChildKey> for Parent<'_, T> { type Container = Alias<T>; }
impl<T, P, K> Registered<There<ChildKey, P>, K> for Parent<'_, T>
where Alias<T>: Registered<P, K> {
    type Value<'a> = <Alias<T> as Registered<P, K>>::Value<'a> where Self: 'a;
}
impl<T: core::fmt::Display> DynRegistered<Here, ValueKey> for Container<T> {
    type Target<'a> = dyn core::fmt::Display + 'a where Self: 'a;
}
"#,
    )
    .expect("write provider");
    let built = compile(
        &provider,
        &["--crate-type=rlib", "--crate-name=scope_provider"],
    );
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let external = format!(
        "scope_provider={}",
        target.join("libscope_provider.rlib").display()
    );
    for (name, source, expected) in [
        (
            "alias_metadata",
            r#"
use scope_provider::*;
use systasis::scoped::*;
fn read<'a>(scope: &SubContainer<'a, String>) -> systasis::Ref<'a, String> {
    Resolve::<'a, Here, ValueKey, op::TryShared>::resolve(scope).unwrap()
}
fn main() {
    let value: <Alias<String> as Registered<Here, ValueKey>>::Value<'_> = String::from("value");
    let _: <Parent<'_, String> as Registered<There<ChildKey>, ValueKey>>::Value<'_> = String::new();
    let _: <Alias<String> as Registered<Here<NamedNamespace>, ValueKey>>::Value<'_> = &value;
    let _: <Alias<String> as Output<Here<NamedNamespace>, ValueKey, op::TryOwned>>::Value<'_> = Some(&value);
    let _: &<Alias<String> as DynRegistered<Here, ValueKey>>::Target<'_> = &value;
    let container = Alias::new(value);
    let guard = { let scope = AsScope::<Restricted>::scope(&container); read(&scope) };
    assert_eq!(&*guard, "value");
}
"#,
            None,
        ),
        (
            "restricted_owned_absent",
            r#"
use scope_provider::*;
use systasis::scoped::*;
fn main() {
    let container = Alias::new(String::new());
    let scope = AsScope::<Restricted>::scope(&container);
    let _ = Resolve::<'_, Here, ValueKey, op::TryOwned>::resolve(&scope);
}
"#,
            Some("E0277"),
        ),
        (
            "backing_private",
            r#"
use scope_provider::*;
use systasis::scoped::*;
fn main() {
    let container = Alias::new(String::new());
    let scope = AsScope::<Restricted>::scope(&container);
    let _ = scope.backing;
}
"#,
            Some("E0616"),
        ),
        (
            "guard_cannot_escape_backing",
            r#"
use scope_provider::*;
use systasis::scoped::*;
fn main() {
    let guard = { let container = Alias::new(String::new());
        let scope = AsScope::<Restricted>::scope(&container);
        Resolve::<'_, Here, ValueKey, op::TryShared>::resolve(&scope).unwrap()
    };
    drop(guard);
}
"#,
            Some("E0597"),
        ),
        (
            "fresh_clone_absent",
            r#"
use systasis::{scoped::*, __private::FreshSlot};
fn main() { let slot = FreshSlot::<String>::new(); let _ = SlotAccess::<op::TryCloneValue>::access(&slot); }
"#,
            Some("E0277"),
        ),
    ] {
        let path = target.join(format!("{name}.rs"));
        fs::write(&path, source).expect("write caller");
        let output = compile(&path, &["--extern", &external, "--error-format=json"]);
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        if let Some(code) = expected {
            assert!(!output.status.success(), "{name} unexpectedly compiled");
            assert!(
                diagnostics
                    .lines()
                    .any(|line| line.contains("\"level\":\"error\"")
                        && line.contains(&format!("\"code\":\"{code}\""))),
                "{name}: {diagnostics}"
            );
        } else {
            assert!(output.status.success(), "{name}: {diagnostics}");
            assert!(
                Command::new(target.join(name))
                    .status()
                    .expect("run alias fixture")
                    .success()
            );
        }
    }
}
