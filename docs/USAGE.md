Statically wired dependency injection with checked, nonblocking value access.

Systasis generates a module-scope `AppContainer` from registrations written
inside an attributed function. Registration annotations name ordinary Rust
types and traits; dependency queries select registrations in the container.
The crate is under development: see the repository README and
`docs/CAPTURE_LIMITS.md` for remaining implementation limits.

User macros inside registrations are deferred for the initial release. This
does not defer systasis's own resolution or registered-type query macros.
Existing working user-macro cases do not imply general support.

For now, `__systasis_*` names are reserved for stable code generation. Do not
introduce them through caller declarations, imports (including globs), or macro
expansions into generated-code scopes. A violation may compile and resolve the
wrong value; this is not a compiler-enforced naming check. Other glob imports
remain allowed. Rust diagnostics may display generated implementation types and
paths instead of public aliases such as `AppContainer`.

## Stored values and fresh constructors

`register_value!` evaluates its initializer once at build time.
`register_type_with!` runs its constructor each time it is resolved;
`register_type!` uses `Default::default()` to do the same.

```rust
use systasis::{app_container::Error, systasis_container};

trait ICount {}
impl ICount for u32 {}

trait ILogger {
    fn text(&self) -> &str;
}
struct Logger(String);
impl ILogger for Logger {
    fn text(&self) -> &str { &self.0 }
}

trait ILabel {}
impl ILabel for String {}

// The generated type can be named outside main().
fn count(container: &AppContainer) -> u32 {
    container.resolve_i_count()
}

#[systasis::container]
fn main() -> Result<(), Error> {
    // Annotate captured state on its binding.
    let label: String = String::from("request");
    let Ok(container) = systasis_container! {
        register_value!(7_u32: u32 as ICount);
        register_value!(Logger(String::from("ready")): Logger as dyn ILogger);
        register_type_with!(String as ILabel, move || label.clone());
    }.build();

    assert_eq!(count(container), 7);
    assert_eq!(container.resolve_i_count(), 7); // Copy: repeatable, no lock.
    assert_eq!(container.resolve_i_label(), "request");
    assert_eq!(container.resolve_i_label(), "request"); // A fresh String.

    let reader = container.try_resolve_i_logger_dyn_ref()?;
    assert_eq!(reader.text(), "ready");
    assert!(matches!(
        container.try_resolve_i_logger(),
        Err(Error::ValueAccessContention)
    ));
    drop(reader);

    // `as dyn ILogger` adds dynamic access; static access remains available.
    let logger: Logger = container.try_resolve_i_logger()?;
    assert_eq!(logger.text(), "ready");
    assert!(matches!(
        container.try_resolve_i_logger_ref(),
        Err(Error::ValueAlreadyConsumed)
    ));
    Ok(())
}
```

`build()` returns `Result<&AppContainer, E>`, not an owned container. Generated
code keeps the pinned owner alive until the enclosing scope exits, preventing
early destruction while resolved values or guards still borrow its contents.
Infallible builds infer the never error type, allowing `let Ok(container) = ...`.
Use `.build::<E>()` when a fallible initializer needs an explicit error type.
A fallible constructor alone does not make build fallible: it has not run yet.

An explicit `try` constructor preserves its annotated return type:

```rust
use std::num::ParseIntError;

trait IPort {}
impl IPort for u16 {}

#[systasis::container]
fn main() -> Result<(), ParseIntError> {
    let configured_port: String = String::from("8080");
    let Ok(container) = systasis::systasis_container! {
        register_type_with!(u16 as IPort, try move || -> Result<u16, ParseIntError> {
            configured_port.parse()
        });
    }.build();
    let port: u16 = container.try_resolve_i_port()?;
    assert_eq!(port, 8080);
    Ok(())
}
```

An annotated `Option<T>` constructor likewise returns `Option<T>`. Constructor
errors are not wrapped in the container's stored-value access error.

### Which resolvers are available?

For a registration under `IValue`, `T` below is its concrete implementation type
and `Error` is `systasis::app_container::Error`. A dash means no such method is
generated. The guard types shown are for synchronized storage; `require(!Sync)`
uses the corresponding `core::cell` guards.

| Registration | By value | Shared borrow | Mutable borrow |
| --- | --- | --- | --- |
| Stored Copy | `resolve_i_value() -> T` | `resolve_i_value_ref() -> &T` | — |
| Stored consumable | `try_resolve_i_value() -> Result<T, Error>` | `try_resolve_i_value_ref() -> Result<Ref<'_, T>, Error>` | `try_resolve_i_value_ref_mut() -> Result<RefMut<'_, T>, Error>` |
| Fresh Default or infallible custom | `resolve_i_value() -> T` | — | — |
| Custom `try` returning `Result<T, E>` | `try_resolve_i_value() -> Result<T, E>` | — | — |
| Custom `try` returning `Option<T>` | `try_resolve_i_value() -> Option<T>` | — | — |

A constructor may return a reference or guard as its `T`; it still has only its
by-value resolver. If constructor wiring borrows a stored consumable registration,
that registration's owned accessor is omitted; its other applicable accessors
remain, subject to runtime contention checks.

For stored values, `as dyn IValue` adds `resolve_i_value_dyn_ref() -> &dyn IValue`
for Copy storage or `try_resolve_i_value_dyn_ref() -> Result<Ref<'_, dyn IValue + '_>, Error>`
for synchronized consumable storage. Static access remains available. The trait
must be dyn-compatible; there is no dyn-mutable accessor or dyn accessor for a
fresh constructor.

### Copy policy in generic code

Concrete stored types select Copy storage automatically when they implement
`Copy`. To select Copy storage for a registered type involving enclosing generic
parameters, write an explicit Copy bound on that whole type: `T: Copy` or
`Wrapper<T>: Copy`.
An indirectly established Copy fact without that explicit bound is diagnosed.
Recognition of equivalent renamed Copy bounds remains an implementation limit.

An unconstrained generic registration stays consumable, even when called with
`u32`. Its resolver API does not change between instantiations:

```rust
use systasis::app_container::Error;

trait IValue {}
impl<T> IValue for T {}

#[systasis::container]
fn run<T>(value: T) -> Result<T, Error> {
    let Ok(container) = systasis::systasis_container! {
        register_value!(value: T as IValue);
    }.build();
    container.try_resolve_i_value()
}

fn main() -> Result<(), Error> {
    assert_eq!(run(7_u32)?, 7);
    assert_eq!(run(String::from("owned"))?, "owned");
    Ok(())
}
```

### Cloning a stored value

Stored `Clone` values support explicit cloning without consuming the original.
Copy storage has `resolve_i_value_clone() -> T`; consumable storage has
`try_resolve_i_value_clone() -> Result<T, Error>`. Both call `Clone::clone`,
including for Copy types. Fresh constructors have no clone accessor.

```rust
use systasis::app_container::Error;

trait ILabel {}
impl ILabel for String {}

#[systasis::container]
fn main() -> Result<(), Error> {
    let Ok(container) = systasis::systasis_container! {
        register_value!(String::from("stored"): String as ILabel);
    }.build();

    let cloned: String = container.try_resolve_i_label_clone()?;
    let original: String = container.try_resolve_i_label()?;
    assert_eq!(cloned, original);
    assert!(matches!(
        container.try_resolve_i_label_clone(),
        Err(Error::ValueAlreadyConsumed)
    ));
    Ok(())
}
```

Cloning a consumable value can also return `ValueAccessContention` while it is
mutably borrowed. Cloning through a dependency query does not itself remove the
original's owned resolver.

### Capturing an array remainder containing references

For now, when destructuring a generic array alias containing references, explicitly
annotate the remainder's exact type before capturing it in `register_type_with!`.
Rust can infer the type locally, but systasis needs this annotation to generate
its storage.

Here, `run` builds the container; `move || tail.len()` is the registered
constructor. The closure captures `tail` directly; no separate registration of
`tail` is needed.

```rust
type Input<'a, T> = [&'a T; 3];

trait ILength {}
impl ILength for usize {}

#[systasis::container]
fn run<'a, T>(input: Input<'a, T>) -> usize {
    let [_, tail @ ..]: Input<'a, T> = input;
    let tail: [&'a T; 2] = tail; // Required annotation for this capture.

    let Ok(container) = systasis::systasis_container! {
        register_type_with!(usize as ILength, move || tail.len());
    }.build();

    container.resolve_i_length()
}

fn main() {
    let values: [u8; 3] = [10, 20, 30];
    assert_eq!(run([&values[0], &values[1], &values[2]]), 2);
}
```

The captured references still borrow the caller's values. This temporary
requirement needs no additional compiler feature.

### Capturing an array reference from a tuple alias

For now, explicitly annotate an array reference extracted from a generic tuple
alias before capturing it in `register_type_with!`. The annotation supplies the
capture's storage type; without it, generated code can fail with a lifetime error.

Here, `run` builds the container; the registered constructor captures `values`,
which borrows the array in `input`. No separate value registration is needed.

```rust
type Input<T> = ([T; 2], bool);

trait ILength {}
impl ILength for usize {}

#[systasis::container]
fn run<T>(input: Input<T>) -> usize {
    let ([ref values @ ..], _): Input<T> = input;
    let values: &[T; 2] = values; // Required annotation for this capture.

    let Ok(container) = systasis::systasis_container! {
        register_type_with!(usize as ILength, move || values.len());
    }.build();

    container.resolve_i_length()
}

fn main() {
    let input: Input<u8> = ([10, 20], false);
    assert_eq!(run(input), 2);
}
```

This temporary requirement adds no lifetime bound or compiler feature.

## Owned dependency injection

Services and constructors remain ordinary Rust. `registered_type!(Interface)`
names the selected implementation inside a registration; `try_resolve!(Interface)`
takes a stored non-Copy value. The dependency is initialized first even when
declared later. No wrapper or extra generic parameter is added to the service.

```rust
use systasis::{app_container::Error, systasis_container};

trait IDatabase {
    fn name(&self) -> &str;
}
struct Database(String);
impl IDatabase for Database {
    fn name(&self) -> &str { &self.0 }
}

struct Service<D: IDatabase> {
    database: D,
}
impl<D: IDatabase> Service<D> {
    fn new(database: D) -> Self { Self { database } }
}
trait IService {
    fn database_name(&self) -> &str;
}
impl<D: IDatabase> IService for Service<D> {
    fn database_name(&self) -> &str { self.database.name() }
}

#[systasis::container]
fn main() -> Result<(), Error> {
    let container = systasis_container! {
        register_value!(
            Service::new(try_resolve!(IDatabase)?):
            Service<registered_type!(IDatabase)> as IService
        );
        register_value!(Database(String::from("application")): Database as IDatabase);
    }.build::<Error>()?;

    assert!(matches!(
        container.try_resolve_i_database(),
        Err(Error::ValueAlreadyConsumed)
    ));
    let service = container.try_resolve_i_service()?;
    assert_eq!(service.database_name(), "application");
    Ok(())
}
```

Final registrations determine dependencies: the last registration of a group in
one namespace wins. Build processes dependency layers, preserving declaration
order within each layer. Missing dependencies and cycles are compile errors.
Queries never fall back to types, traits or aliases outside the selected container.

## Trait groups and dynamic access

`as IRead + ILength` registers one value under the complete trait group, not
separate values under each trait. Queries must name the whole group; their trait
order does not matter. Generated method names alphabetize the trait names.

`as dyn IRead + ILength` additionally provides a shared dynamic accessor for
the group, while keeping concrete storage and static accessors:

```rust
use systasis::app_container::Error;

trait IRead {
    fn text(&self) -> &str;
}
trait ILength {
    fn length(&self) -> usize;
}
impl IRead for String {
    fn text(&self) -> &str { self.as_str() }
}
impl ILength for String {
    fn length(&self) -> usize { self.len() }
}
trait IObserved {}
impl IObserved for usize {}

#[systasis::container]
fn main() -> Result<(), Error> {
    let container = systasis::systasis_container! {
        register_value!({
            let view = try_resolve_dyn_ref!(ILength + IRead)?;
            let target: &resolve_type!(dyn IRead + ILength) = &*view;
            target.length()
        }: usize as IObserved);
        register_value!(String::from("group"): String as dyn IRead + ILength);
    }.build::<Error>()?;

    assert_eq!(container.resolve_i_observed(), 5);
    let dynamic = container.try_resolve_i_length_i_read_dyn_ref()?;
    assert_eq!(dynamic.text(), "group");
    let concrete = container.try_resolve_i_length_i_read_ref()?;
    assert_eq!(&*concrete, "group");
    drop(concrete);
    drop(dynamic);
    assert_eq!(container.try_resolve_i_length_i_read()?, "group");
    Ok(())
}
```

Systasis generates a combined trait for the dynamic target; inside registrations,
`resolve_type!(dyn IRead + ILength)` names it. These combined targets are specific
to their container definition. Without `dyn`, `resolve_type!(IRead + ILength)`
and `registered_type!(IRead + ILength)` name the concrete implementation instead.
All opted-in traits must be dyn-compatible, including any required associated
type bindings. A group and separately registered individual traits are independent.

## Local namespaces

Use `in name` to register another implementation under the same interface in a
local namespace. Inside registrations, `_from` queries select that namespace;
public methods append `_in_name`. Omitting the namespace selects `default`.

```rust
trait ICount {}
impl ICount for u32 {}
trait ITotal {}
impl ITotal for u32 {}

#[systasis::container]
fn main() {
    let Ok(container) = systasis::systasis_container! {
        register_value!(7_u32: u32 as ICount);
        register_value!(11_u32: u32 as ICount in test);
        register_type_with!(u32 as ITotal, || {
            resolve!(ICount) + resolve_from!(ICount, test)
        });
    }.build();

    assert_eq!(container.resolve_i_count(), 7);
    assert_eq!(container.resolve_i_count_in_default(), 7);
    assert_eq!(container.resolve_i_count_in_test(), 11);
    assert_eq!(container.resolve_i_total(), 18);
}
```

The namespaces have independent registrations and overrides. A missing named
registration does not fall back to the default namespace. The corresponding
type query is `resolve_type_from!(ICount, test)`.

## Subcontainers and scoped injection

`register_container!(primary: &ChildContainer)` borrows an existing container
under the path `primary`; it does not import that container's registrations into
the parent's default namespace. `ChildContainer` may be a Rust type alias.
Children need names: `register_container!(default: ...)` is rejected.

Each container definition generates its own `AppContainer`. Separate modules
keep those type names distinct. In this example, `main` builds the database
container and `application::run` builds a container that uses it.

```rust
use systasis::app_container::Error;

trait IDatabase {}
impl IDatabase for String {}

mod application {
    use super::Error;

    trait ILength {}
    impl ILength for usize {}

    // This component receives only the database scope.
    fn database_length(database: &primary::SubContainer<'_>) -> Result<usize, Error> {
        Ok(database.try_resolve_i_database_ref()?.len())
    }

    #[systasis::container]
    pub fn run(primary: &super::AppContainer) -> Result<(), Error> {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &super::AppContainer);
            register_type_with!(usize as ILength, try || -> Result<usize, Error> {
                Ok(try_resolve_ref_from!(IDatabase, primary)?.len())
            });
        }.build();

        assert_eq!(database_length(container.primary())?, 11);
        assert_eq!(container.try_resolve_i_length()?, 11);
        Ok(())
    }
}

#[systasis::container]
fn main() -> Result<(), Error> {
    let Ok(database) = systasis::systasis_container! {
        register_value!(String::from("application"): String as IDatabase);
    }.build();
    application::run(database)
}
```

`container.primary()` returns `&primary::SubContainer<'_>`, a generated type
that exposes the child's permitted resolvers, not its unrestricted backing
container. Here the constructor borrows `IDatabase`, so the scope has no owned
`try_resolve_i_database()` method. Shared and mutable borrowed access remain
available, with contention checked while a guard is held. No `IDatabase` trait
import is needed inside `application` for its registration-name query.

Nested scopes preserve these restrictions. Inside a registration, use
`try_resolve_ref_from!(IDatabase, branch::primary)`; outside, use
`container.branch().primary().try_resolve_i_database_ref()`. Two instances of
one child type can be composed under different names. The child owners must
remain alive while their composed scopes are used.

## Borrowing and thread-safety choices

With synchronized storage, `try_resolve_i_logger_ref()` returns a [`Ref`] and
`try_resolve_i_logger_ref_mut()` returns a [`RefMut`], each inside `Result`.
These are systasis guards: they retain shared or exclusive access and dereference
to the present value. Incompatible access returns `Error::ValueAccessContention`
immediately; access after consumption returns `Error::ValueAlreadyConsumed`.
Stored Copy values instead expose ordinary shared references and repeatable
by-value resolution, with no mutable accessor.

Natural `Send`/`Sync` implementations depend on stored state. Optional
`#[systasis::container(require(Send, Sync))]` checks both traits; either may be
requested independently. Positive requirements do not select a storage mode.

`#[systasis::container(require(!Sync))]` instead selects non-atomic `RefCell`
borrow tracking for mutable/takeable values and makes the container `!Sync`.
Its returned `core::cell::Ref` / `core::cell::RefMut` guards are `!Send` and
`!Sync`; retaining one across `.await`
therefore prevents that future from being `Send`. Other mutable/takeable values
use `parking_lot` with `send_guard` in std builds, or Spin in no_std builds.
Their guards retain the corresponding payload-dependent auto traits.

Returned constructor values may borrow captures or retain dependency guards.
Storing a service that borrows another registration inside the same container
is deferred; it is not established by the owned-injection example above.

## Features

- `std` is enabled by default. Disable default features for the no_std runtime;
  no allocator is required by systasis itself.
- `portable-atomic` enables Spin's portable atomics and critical-section
  fallback. The application supplies any required platform critical-section
  implementation; systasis does not assume a single core.
- `experimental-hardware` enables that integration and its embedded compile/link
  checks. Execution on physical boards remains unverified.
- `resolve_unchecked` enables unsafe nonblocking accessors. These retain borrow
  protection and ownership exclusions; callers must guarantee availability and
  successful acquisition at the call. Checked accessors remain unchanged.
