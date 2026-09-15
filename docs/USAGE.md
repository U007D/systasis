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
    // Captured locals require an explicit binding type.
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
