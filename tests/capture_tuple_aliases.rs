//! Capture projections preserve ordinary tuple aliases and borrow modes.
#![forbid(unsafe_code)]
use std::cell::Cell;
trait IText {}
impl IText for &str {}
trait ILength {}
impl ILength for usize {}

mod borrowed {
    use super::*;
    type Shared<'a> = &'a ((String, usize), bool);
    fn receive<'a>(container: &AppContainer<'a>) -> &'a str {
        container.resolve_i_text()
    }
    #[systasis::container(require(Send, Sync))]
    fn run<'a>(input: Shared<'a>, call: impl FnOnce(&'a str)) {
        let ((head, _), _): Shared<'a> = input;
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(&'a str as IText, || head.as_str());
        }
        .build();
        call(receive(container));
    }
    #[test]
    fn nested_borrowed_alias_has_nameable_container_and_returned_reference() {
        let value = ((String::from("borrowed"), 0), true);
        run(&value, |text| assert_eq!(text, "borrowed"));
        assert_eq!(value.0.0, "borrowed");
    }
}

struct Tracked<'a>(&'a Cell<usize>);
impl Drop for Tracked<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
mod owned {
    use super::*;
    type Pair<'a> = (Tracked<'a>, Tracked<'a>);
    #[systasis::container]
    fn run<'a>(input: Pair<'a>, drops: &'a Cell<usize>) {
        let (head, tail): Pair<'a> = input;
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(usize as ILength, || head.0.get());
        }
        .build();
        drop(tail);
        assert_eq!(container.resolve_i_length(), 1);
        assert_eq!(drops.get(), 1);
    }
    #[test]
    fn only_selected_owned_alias_element_moves_into_capture_storage() {
        let drops = Cell::new(0);
        run((Tracked(&drops), Tracked(&drops)), &drops);
        assert_eq!(drops.get(), 2);
    }
}

mod nested_owned {
    use super::*;
    type Pair<'a> = (Tracked<'a>, Tracked<'a>);
    type Envelope<'a> = (Pair<'a>, bool);
    #[systasis::container]
    fn run<'a>(input: Envelope<'a>, drops: &'a Cell<usize>) {
        let ((head, tail), _): Envelope<'a> = input;
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(usize as ILength, || head.0.get());
        }
        .build();
        drop(tail);
        assert_eq!(container.resolve_i_length(), 1);
        assert_eq!(drops.get(), 1);
    }
    #[test]
    fn nested_owned_aliases_move_only_the_selected_leaf() {
        let drops = Cell::new(0);
        run(((Tracked(&drops), Tracked(&drops)), true), &drops);
        assert_eq!(drops.get(), 2);
    }
}

mod modes {
    use super::*;
    type Tuple = (String, u8);
    type SharedShared<'a, 'b> = &'a &'b Tuple;
    type SharedMutable<'a, 'b> = &'a &'b mut Tuple;
    type MutableShared<'a, 'b> = &'a mut &'b Tuple;
    type MutableMutable<'a, 'b> = &'a mut &'b mut Tuple;
    type Explicit<'a> = &'a (u8, bool);
    #[systasis::container]
    fn run<'a, 'b>(
        ss: SharedShared<'a, 'b>,
        sm: SharedMutable<'a, 'b>,
        ms: MutableShared<'a, 'b>,
        mm: MutableMutable<'a, 'b>,
        explicit: Explicit<'a>,
    ) {
        let (a, _): SharedShared<'a, 'b> = ss;
        let (b, _): SharedMutable<'a, 'b> = sm;
        let (c, _): MutableShared<'a, 'b> = ms;
        let (d, _): MutableMutable<'a, 'b> = mm;
        let &(copied, _): Explicit<'a> = explicit;
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(usize as ILength, || a.len() + b.len() + c.len() + d.len() + usize::from(copied));
        }.build();
        assert_eq!(container.resolve_i_length(), 9);
    }
    #[test]
    fn nested_alias_reference_modes_and_explicit_reference_patterns() {
        let first = (String::from("a"), 0);
        let mut second = (String::from("bb"), 0);
        let third = (String::from("ccc"), 0);
        let mut fourth = (String::from("dd"), 0);
        run(
            &&first,
            &&mut second,
            &mut &third,
            &mut &mut fourth,
            &(1, false),
        );
    }
}

mod overridden {
    use super::*;
    type Alias = (std::rc::Rc<usize>, bool);
    #[systasis::container(require(Send, Sync))]
    #[test]
    fn discarded_alias_capture_does_not_move_or_constrain_state() {
        let (head, _): Alias = (std::rc::Rc::new(8), false);
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(usize as ILength, || *head);
            register_type_with!(usize as ILength, || 3);
        }
        .build();
        assert_eq!(container.resolve_i_length(), 3);
        assert_eq!(*head, 8);
        assert_eq!(std::rc::Rc::strong_count(&head), 1);
    }
}

mod generic {
    use super::*;
    type Pair<T> = (T, bool);
    fn receive<T: AsRef<str> + Send + Sync>(container: &AppContainer<T>) -> usize {
        container.resolve_i_length()
    }
    #[systasis::container(require(Send, Sync))]
    fn run<T: AsRef<str> + Send + Sync>(input: Pair<T>) {
        let (head, _): Pair<T> = input;
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(usize as ILength, || head.as_ref().len());
        }
        .build();
        assert_eq!(receive(container), 4);
    }
    #[test]
    fn generic_alias_keeps_only_authored_container_parameters() {
        run((String::from("four"), true));
        run(("four", false));
    }
}

mod explicit_modes {
    use super::*;
    type Pair = (String, String);
    #[systasis::container]
    fn run(mut input: Pair) {
        let (ref head, ref mut tail): Pair = input;
        tail.push('!');
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(usize as ILength, || head.len() + tail.len());
        }
        .build();
        assert_eq!(container.resolve_i_length(), 6);
    }
    #[test]
    fn explicit_ref_and_ref_mut_alias_bindings_keep_their_backing_values() {
        run((String::from("one"), String::from("ab")));
    }
}

mod const_lifetime {
    use super::*;
    type Pair<'a, const N: usize> = (&'a str, [u8; N]);
    fn receive<'a, const N: usize>(container: &AppContainer<'a, N>) -> &'a str {
        container.resolve_i_text()
    }
    #[systasis::container(require(Send, Sync))]
    fn run<'a, const N: usize>(input: Pair<'a, N>) {
        let (head, tail): Pair<'a, N> = input;
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(&'a str as IText, || head);
            register_type_with!(usize as ILength, || tail.len());
        }
        .build();
        assert_eq!(receive(container), "input");
        assert_eq!(container.resolve_i_length(), N);
    }
    #[test]
    fn capture_records_preserve_original_const_and_lifetime_parameters() {
        let text = String::from("input");
        run((text.as_str(), []));
        run((text.as_str(), [1, 2, 3]));
    }
}

mod hygiene {
    use super::*;
    type Pair = (String, bool);
    struct __CaptureOwned;
    struct __CaptureRecord0;
    trait __SystasisCaptureTuple2_0 {}
    impl __SystasisCaptureTuple2_0 for __CaptureOwned {}
    #[systasis::container]
    #[test]
    fn helper_names_do_not_capture_surrounding_symbols() {
        let _: &dyn __SystasisCaptureTuple2_0 = &__CaptureOwned;
        let _ = __CaptureRecord0;
        let (__systasis_captures, _): Pair = (String::from("input"), false);
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(usize as ILength, || __systasis_captures.len());
        }
        .build();
        assert_eq!(container.resolve_i_length(), 5);
    }
}

#[test]
#[cfg(not(miri))]
fn exported_container_hides_private_capture_types_across_crates() {
    use std::{fs, path::Path, process::Command};
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = root
        .join("target/capture-tuple-aliases")
        .join(if cfg!(feature = "std") {
            "std"
        } else {
            "no-std"
        });
    let mut build = Command::new(env!("CARGO"));
    build
        .current_dir(root)
        .args(["build", "--lib", "--offline", "--locked", "--target-dir"])
        .arg(&target);
    if !cfg!(feature = "std") {
        build.arg("--no-default-features");
    }
    let output = build.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let provider = r#"
#![forbid(unsafe_code)]
use std::cell::Cell;
mod private {
    pub struct Secret<'a>(pub &'a Cell<usize>);
    use super::Cell;
    pub type Tuple<'a> = (Secret<'a>, bool);
}
type Alias<'a> = private::Tuple<'a>;
trait ILength {} impl ILength for usize {}
#[systasis::container]
pub fn run<'a>(value: &'a Cell<usize>, call: impl FnOnce(&AppContainer<'a>)) {
    let (head, _): Alias<'a> = (private::Secret(value), false);
    let Ok(container) = systasis::systasis_container! {
        register_type_with!(usize as ILength, || head.0.get());
    }.build();
    call(container);
}
"#;
    let consumer = r#"
#![forbid(unsafe_code)]
use provider::AppContainer as Renamed;
type Alias<'a> = Renamed<'a>;
fn receive(container: &Alias<'_>) -> usize { container.resolve_i_length() }
fn main() { provider::run(&std::cell::Cell::new(17), |container| assert_eq!(receive(container),17)); }
"#;
    for (name, source) in [("provider", provider), ("consumer", consumer)] {
        let path = target.join(format!("{name}.rs"));
        fs::write(&path, source).unwrap();
        let mut compiler = Command::new("rustc");
        compiler
            .args(["--edition=2024", "-Dwarnings"])
            .arg(&path)
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
        if name == "provider" {
            compiler.arg("--crate-type=rlib");
        } else {
            compiler.arg("--extern").arg(format!(
                "provider={}",
                target.join("libprovider.rlib").display()
            ));
        }
        let output = compiler.output().unwrap();
        fs::write(target.join(format!("{name}.stderr")), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(
        Command::new(target.join("consumer"))
            .status()
            .unwrap()
            .success()
    );
    let private = target.join("private.rs");
    fs::write(
        &private,
        "type Hidden = provider::__systasis_injected::__CaptureOwned; fn main() {}",
    )
    .unwrap();
    let output = Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata", "--error-format=short"])
        .arg(private)
        .arg("--out-dir")
        .arg(&target)
        .arg("--extern")
        .arg(format!(
            "provider={}",
            target.join("libprovider.rlib").display()
        ))
        .arg("-L")
        .arg(format!(
            "dependency={}",
            target.join("debug/deps").display()
        ))
        .output()
        .unwrap();
    fs::write(target.join("private.stderr"), &output.stderr).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success()
            && stderr.contains("E0603")
            && stderr.contains("module `__systasis_injected` is private"),
        "{stderr}"
    );
}
