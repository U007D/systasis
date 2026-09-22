//! Downstream callers name aliases using only declaration-site parameters.
#![cfg(not(miri))]
#![forbid(unsafe_code)]

#[cfg(all(test, not(miri)))]
mod support;

use std::{fs, path::Path, process::Command};

fn compile(artifacts: &support::Artifacts, target: &Path, source: &Path, arguments: &[&str]) {
    let output = artifacts
        .rustc()
        .args(["--edition=2024", "--out-dir"])
        .arg(target)
        .arg(source)
        .args(arguments)
        .output()
        .expect("compile cross-crate fixture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn exported_alias_preserves_generic_type_const_and_lifetime_parameters() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let backend = if cfg!(feature = "std") {
        "std"
    } else {
        "no-std"
    };
    let target = root.join("target/generic-cross-crate").join(backend);
    let mut build = Command::new(env!("CARGO"));
    build
        .current_dir(root)
        .args(["build", "--lib", "--offline", "--locked", "--target-dir"])
        .arg(&target);
    if !cfg!(feature = "std") {
        build.arg("--no-default-features");
    }
    let artifacts = support::Artifacts::build(&mut build);

    let library = target.join("generic_provider.rs");
    fs::write(
        &library,
        r#"
#![no_std]
#![forbid(unsafe_code)]
#![allow(unused_parens)]
pub trait IValue {}
impl<T> IValue for T {}
pub mod inferred_value {
    #[systasis::container]
    pub fn run(value: u32, use_container: impl FnOnce(&AppContainer)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value as crate::IValue);
        }.build();
        use_container(container);
    }
}
pub mod grouped {
    #[systasis::container]
    pub fn run<T: Copy>(value: T, use_container: impl FnOnce(&AppContainer<T>)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: (T) as crate::IValue);
        }.build();
        use_container(container);
    }
}
pub trait IArray {}
impl<T, const N: usize> IArray for [T; N] {}
pub mod arrays {
    #[systasis::container]
    pub fn run<T, const N: usize>(value: [T; N], use_container: impl FnOnce(&AppContainer<T, N>))
    where [T; N]: Copy {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: [T; N] as crate::IArray);
        }.build();
        use_container(container);
    }
}
pub trait IBorrow {}
pub mod private_factory {
    struct Service;
    trait IService {}
    impl IService for Service {}
    #[systasis::container]
    pub fn run(use_container: impl FnOnce(&AppContainer)) {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(Service as IService, || Service);
        }.build();
        use_container(container);
    }
}
impl<T: ?Sized> IBorrow for &T {}
pub mod borrowed {
    #[systasis::container]
    pub fn run<'a, T: ?Sized>(value: &'a T, use_container: impl FnOnce(&AppContainer<'a, T>))
    where &'a T: Copy {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: &'a T as crate::IBorrow);
        }.build();
        use_container(container);
    }
}
"#,
    )
    .expect("write provider fixture");
    compile(
        &artifacts,
        &target,
        &library,
        &["--crate-type=rlib", "--crate-name=generic_provider"],
    );

    let caller = target.join("generic_caller.rs");
    fs::write(
        &caller,
        r#"
#![forbid(unsafe_code)]
fn inspect_inferred(container: &generic_provider::inferred_value::AppContainer) {
    assert_eq!(container.resolve_i_value(), 11);
    assert_eq!(container.resolve_i_value(), 11);
}
fn inspect_grouped<T: Copy>(container: &generic_provider::grouped::AppContainer<T>) {
    let _: T = container.resolve_i_value();
    let _: &T = container.resolve_i_value_ref();
    let _: T = container.resolve_i_value();
}
fn inspect_array<T, const N: usize>(container: &generic_provider::arrays::AppContainer<T, N>)
where [T; N]: Copy {
    let _: [T; N] = container.resolve_i_array();
    let _: &[T; N] = container.resolve_i_array_ref();
}
fn inspect_borrow<'a, T: ?Sized>(container: &generic_provider::borrowed::AppContainer<'a, T>)
where &'a T: Copy {
    let _: &'a T = container.resolve_i_borrow();
}
mod composed_array {
    use generic_provider::arrays::AppContainer as Imported;
    type Alias<T, const N: usize> = Imported<T, N>;
    trait ISelected {}
    impl<T, const N: usize> ISelected for [T; N] {}
    fn receive<T, const N: usize>(scope: &child::SubContainer<'_, T, N>) -> [T; N]
    where [T; N]: Copy { scope.resolve_i_array() }
    #[systasis::container]
    pub fn run<T, const N: usize>(child: &Alias<T, N>)
    where [T; N]: Copy + PartialEq {
        let Ok(container) = systasis::systasis_container! {
            register_container!(child: &Alias<T, N>);
            register_value!(resolve_from!(crate::IArray, child): resolve_type_from!(crate::IArray, child) as ISelected);
        }.build();
        assert!(receive(container.child()) == container.resolve_i_selected());
    }
}
mod composed_borrow {
    use generic_provider::borrowed::AppContainer as Imported;
    type Alias<'a, T> = Imported<'a, T>;
    trait ISelected {}
    impl<T: ?Sized> ISelected for &T {}
    fn receive<'a, T: ?Sized>(scope: &child::SubContainer<'_, 'a, T>) -> &'a T
    where &'a T: Copy { scope.resolve_i_borrow() }
    #[systasis::container]
    pub fn run<'a, T: ?Sized>(child: &Alias<'a, T>)
    where &'a T: Copy {
        let Ok(container) = systasis::systasis_container! {
            register_container!(child: &Alias<'a, T>);
            register_value!(resolve_from!(crate::IBorrow, child): resolve_type_from!(crate::IBorrow, child) as ISelected);
        }.build();
        assert!(core::ptr::eq(receive(container.child()), container.resolve_i_selected()));
    }
}
fn main() {
    generic_provider::inferred_value::run(11, inspect_inferred);
    generic_provider::grouped::run(7u32, inspect_grouped);
    fn inspect_private_factory(_: &generic_provider::private_factory::AppContainer) {}
    generic_provider::private_factory::run(inspect_private_factory);
    generic_provider::arrays::run([1u32, 2, 3], inspect_array);
    let owned = String::from("borrowed");
    generic_provider::borrowed::run(owned.as_str(), inspect_borrow);
    generic_provider::arrays::run([1u32, 2, 3], composed_array::run);
    generic_provider::borrowed::run(owned.as_str(), composed_borrow::run);
}
"#,
    )
    .expect("write caller fixture");
    let provider = format!(
        "generic_provider={}",
        target.join("libgeneric_provider.rlib").display()
    );
    compile(&artifacts, &target, &caller, &["--extern", &provider]);
    let output = Command::new(target.join("generic_caller"))
        .output()
        .expect("run cross-crate caller");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
