Statically wired dependency injection with checked, nonblocking value access.

`systasis_container!` declares a public `SystasisContainer` and a parameterless
`SystasisContainer::build()` method. Registration annotations name ordinary Rust
types and traits; dependency queries select registrations in the container.
The existing attributed-function form remains available for runtime inputs.
The crate is under development: see the repository README and
`docs/CAPTURE_LIMITS.md` for remaining implementation limits.

The current procedural-macro crate requires nightly Rust. Use the
`nightly` toolchain; application crates need no extra feature
attributes for generated containers. The repository's `docs/NIGHTLY.md` records
the unstable features and their purposes. A consuming project's
`rust-toolchain.toml` can select the channel:

```toml
[toolchain]
channel = "nightly"
profile = "minimal"
```

User macros inside registrations are deferred for the initial release. This
does not defer systasis's own resolution or registered-type query macros.
Existing working user-macro cases do not imply general support.

For now, `__systasis_*` names are reserved for stable code generation. Do not
introduce them through caller declarations, imports (including globs), or macro
expansions into generated-code scopes. A violation may compile and resolve the
wrong value; this is not a compiler-enforced naming check. Other glob imports
remain allowed. Rust diagnostics may display generated implementation types and
paths instead of public aliases such as `SystasisContainer`.

## Declaration containers

Place the declaration at module or block scope. No enclosing function attribute
is needed. Building returns an owned container; each call initializes a new one.

```rust
use systasis::systasis_container;

systasis_container! {
    register_value!(42: u8 as Copy);
}

pub fn init_container() -> SystasisContainer {
    let Ok(container) = SystasisContainer::build();
    container
}

fn main() {
    assert_eq!(init_container().resolve_copy(), 42);
}
```

Initializers execute during `build()`, not at the declaration. Their expressions
can use items in scope, including functions and constants. As with an ordinary
associated function, this parameterless `build()` does not capture surrounding
runtime variables; use the attribute form below when supplying such inputs.

### Inferred initialization errors

Propagate an initializer's error with `?`. Systasis infers and combines the source
errors in `SystasisContainerError`; no error-type annotation or list is needed.
Sources must implement `core::error::Error + 'static`.

```rust
use systasis::systasis_container;

trait IPort {}
impl IPort for u16 {}
trait IEnabled {}
impl IEnabled for bool {}

systasis_container! {
    register_value!("8080".parse::<u16>()?: u16 as IPort);
    register_value!("true".parse::<bool>()?: bool as IEnabled);
}

pub fn init_container() -> Result<SystasisContainer, SystasisContainerError> {
    SystasisContainer::build()
}

fn main() -> Result<(), SystasisContainerError> {
    let container = init_container()?;
    assert_eq!(container.resolve_i_port(), 8080);
    assert!(container.resolve_i_enabled());
    Ok(())
}
```

The generated enum owns errors inline, without boxing. `Error::source()` exposes
the original error, and `Display` forwards its message. `Send`/`Sync` follow the
source types. Internal variants and source-type parameters are not public naming
contracts; use the generated error name and the `Error` trait.

Infallible declarations infer the never error type, permitting the first
example's irrefutable `let Ok(...)`. Errors handled inside a nested closure, or
returned by a lazy constructor during resolution, do not make building fallible.
Failed builds drop initialized values before returning the error, except values
owned by the error itself. No partial container is exposed.

## Function-local inputs (attribute form)

`register_value!` evaluates its initializer once at build time.
`register_type_with!` runs its constructor each time it is resolved;
`register_type!` does the same through `Default::default()` when its type
implements `Default`; otherwise it supplies type lookup only.

Supply the stored value's type before `as`, for example
`register_value!(String::new(): String as ILabel);`. Missing type information
produces a compile-time error showing where to add the annotation.

```rust
use systasis::{container::Error, systasis_container};

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
fn count(container: &SystasisContainer) -> u32 {
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

    assert_eq!(count(&container), 7);
    assert_eq!(container.resolve_i_count(), 7); // Copy: repeatable, no lock.
    assert_eq!(container.resolve_i_label(), "request");
    assert_eq!(container.resolve_i_label(), "request"); // A fresh String.

    let logger: Logger = container.try_resolve_i_logger()?;
    let reader: &dyn ILogger = &logger; // The caller explicitly borrows its value.
    assert_eq!(reader.text(), "ready");
    assert!(matches!(
        container.try_resolve_i_logger(),
        Err(Error::ValueAlreadyConsumed)
    ));
    Ok(())
}
```

In this attribute form, `.build()` returns `Result<SystasisContainer, E>`. The
caller owns the container and can return it from its initialization function.
Pass `&container` to functions
accepting a shared container reference. Stored values resolve by value, including
stored references, whose referents must outlive them. Constructor outputs may
still borrow their captured state; those borrows prevent moving or dropping the
container while usable. Borrowing another stored registration remains deferred.
Infallible builds infer the never error type, allowing `let Ok(container) = ...`.
Use `.build::<E>()` when a fallible initializer needs an explicit error type.
A fallible constructor alone does not make build fallible: it has not run yet.

You can hold or move the builder before building. It has no resolution methods;
`build()` consumes it, so it cannot build twice. Dropping it without building
skips stored initializers, drops owned inputs and releases borrowed inputs.

```rust
use std::cell::Cell;

trait ICount {}
impl ICount for u32 {}

#[systasis::container]
fn main() {
    let calls: Cell<u32> = Cell::new(0);
    let builder = systasis::systasis_container! {
        register_value!({ calls.set(calls.get() + 1); 7_u32 }: u32 as ICount);
    };
    assert_eq!(calls.get(), 0);
    let moved_builder = builder;
    let Ok(container) = moved_builder.build();
    assert_eq!(calls.get(), 1);
    assert_eq!(container.resolve_i_count(), 7);
}
```

An initializer can propagate its own error during building. Here parsing fails
before a container is published; the explicit build error type requires no
enclosing `Result` return type:

```rust
use std::num::ParseIntError;

trait IPort {}
impl IPort for u16 {}

#[systasis::container]
fn main() {
    let configured_port: String = String::from("not a port");
    let built = systasis::systasis_container! {
        register_value!(configured_port.parse::<u16>()?: u16 as IPort);
    }.build::<ParseIntError>();

    assert!(built.is_err());
    assert_eq!(configured_port, "not a port"); // The initializer only borrowed it.
}
```

On failure, initialized values and owned captures are dropped before the error
is returned, except resources deliberately transferred into the error. Temporary
build-only borrows end, and no partial container is exposed. Systasis does not
undo caller side effects or restore values already consumed from an independent
child container. Similarly, a failed resolution does not roll back earlier
successful dependency consumption.

In the attribute form, error types can be selected with `.build::<E>()` or
inferred from caller context; `.build::<_>()` also requests inference.
Infallible builds default to the never
error type. A fallible initializer may need an explicit `E`; errors are not
automatically combined into a generated enum.

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

### Registering a type without constructing a value

`register_type!(T as IT)` permits `resolve_type!(IT)` and `registered_type!(IT)`
without `T: Default` or a constructor. This is useful when the type configures
another registration, such as a channel's message type:

```rust
use std::sync::mpsc;

struct Message(u8); // No Default implementation.
trait IMessage {}
impl IMessage for Message {}
trait ISender {}
impl<T> ISender for mpsc::Sender<T> {}
trait IReceiver {}
impl<T> IReceiver for mpsc::Receiver<T> {}

#[systasis::container]
fn main() {
    let (tx, rx) = mpsc::channel();
    let Ok(container) = systasis::systasis_container! {
        register_type!(Message as IMessage);
        register_value!(tx: mpsc::Sender<resolve_type!(IMessage)> as ISender);
        register_value!(rx: mpsc::Receiver<resolve_type!(IMessage)> as IReceiver);
    }.build();

    let sender = container.resolve_i_sender_clone();
    let receiver = container.try_resolve_i_receiver().unwrap();
    assert!(sender.send(Message(42)).is_ok());
    assert_eq!(receiver.recv().unwrap().0, 42);
}
```

No `Message` is created by registration, building or type lookup. Because it has
no constructor, this container has no callable `resolve_i_message()` method;
attempting value resolution fails at compile time, not at runtime. To enable
value resolution, implement `Default` or replace the registration with
`register_type_with!` supplying a constructor. In generic code, default value
resolution requires a `Default` bound; type lookup does not.

### Which resolvers are available?

For a registration under `IValue`, `T` is its concrete implementation type
and `Error` is `systasis::container::Error`.

| Registration | Resolver | Try resolver |
| --- | --- | --- |
| Stored Copy | `resolve_i_value() -> T` | `try_resolve_i_value() -> Result<T, !>` |
| Stored Clone, not Copy | `resolve_i_value_clone() -> T` | `try_resolve_i_value_clone() -> Result<T, !>` |
| Stored neither | — | `try_resolve_i_value() -> Result<T, Error>` |
| Fresh Default or infallible custom | `resolve_i_value() -> T` | `try_resolve_i_value() -> Result<T, !>` |
| Type registration without Default or a constructor | — | — |
| Custom `try` returning `Result<T, E>` | — | `try_resolve_i_value() -> Result<T, E>` |
| Custom `try` returning `Option<T>` | — | `try_resolve_i_value() -> Option<T>` |

Copy and Clone results are repeatable; move-only values transfer once.
Clone-only storage has no consuming resolver. No registration gets a borrowed
`_ref` or `_ref_mut` accessor, including dyn or unchecked variants.
A constructor can itself return a reference as its output type.

Infallible try methods use the actual never type `!`: `let Ok(value) = ...;`
is irrefutable. Using `?` in a function returning `Result<_, E>` still requires
`E: From<!>`; use the direct resolver or `let Ok(...)` when no conversion exists.

For stored values, `as dyn IValue` validates dyn compatibility and enables
`resolve_type!(dyn IValue)`. It does not add a borrowed resolver. Borrow or
coerce the resolved concrete value explicitly when trait-object access is needed.

### Copy and Clone policy in generic code

Concrete stored types select Copy first, otherwise Clone, otherwise move-only
storage. For a type involving enclosing generic parameters, an explicit bound
on the whole registered type selects its policy: `T: Copy`, `Wrapper<T>: Copy`,
`T: Clone`, or `Wrapper<T>: Clone`. A Clone bound without a Copy bound gives
the clone API even when the caller instantiates it with a Copy type.
Shared references are intrinsically Copy regardless of their referent's bounds.

An indirectly established Copy/Clone fact without the explicit whole-type bound
is diagnosed. Parentheses do not change the type. Renamed trait imports and
equivalent types spelled through different aliases retain recognition gaps;
these are implementation limits, not different policies.

An unconstrained generic registration stays consumable, even when called with
`u32`. Its resolver API does not change between instantiations:

```rust
use systasis::container::Error;

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

Stored Clone-only values use immutable storage without locks. Both clone
resolvers invoke `Clone::clone`; neither consumes the original. Copy resolution
does not call Clone, and Copy/fresh registrations have no clone resolver.

```rust
trait ILabel {}
impl ILabel for String {}

#[systasis::container]
fn main() {
    let Ok(container) = systasis::systasis_container! {
        register_value!(String::from("stored"): String as ILabel);
    }.build();

    let mut first: String = container.resolve_i_label_clone();
    first.push('!');
    let Ok(second) = container.try_resolve_i_label_clone();
    assert_eq!(first, "stored!");
    assert_eq!(second, "stored");
}
```

The never error means resolution cannot fail through consumption or contention.
Caller-written Clone implementations can still panic or allocate.

### Stored references

A stored `&T` follows the Copy API. A stored `&mut T` follows the move-once
API: resolution transfers the reference, without reborrowing or retaining a guard.

```rust
use systasis::container::Error;

trait IText {}
impl IText for &str {}
trait ICounter {}
impl ICounter for &mut u32 {}

#[systasis::container]
fn init<'a>(text: &'a str, counter: &'a mut u32) -> SystasisContainer<'a> {
    let Ok(container) = systasis::systasis_container! {
        register_value!(text: &'a str as IText);
        register_value!(counter: &'a mut u32 as ICounter);
    }.build();
    container
}

fn main() -> Result<(), Error> {
    let text = String::from("external");
    let mut count = 0;
    let container = init(&text, &mut count);
    let counter = container.try_resolve_i_counter()?;
    let Ok(label) = container.try_resolve_i_text();
    assert_eq!(label, "external");
    assert!(matches!(container.try_resolve_i_counter(), Err(Error::ValueAlreadyConsumed)));
    drop(container);
    *counter += 1; // The transferred reference does not borrow the container.
    assert_eq!(count, 1);
    Ok(())
}
```

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
takes a stored move-only value. `resolve_clone!(Interface)` clones Clone-only storage. The dependency is initialized first even when
declared later. No wrapper or extra generic parameter is added to the service.

```rust
use systasis::{container::Error, systasis_container};

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
Both value- and type-query cycle errors show the closed registration path;
registrations merely waiting on the cycle are omitted.
Queries never fall back to types, traits or aliases outside the selected container.

## Trait groups and dynamic access

`as IRead + ILength` registers one value under the complete trait group, not
separate values under each trait. Queries must name the whole group; their trait
order does not matter. Generated method names alphabetize the trait names.

`as dyn IRead + ILength` additionally provides a combined trait-object type
query, while keeping concrete storage and ordinary value accessors:

```rust
use systasis::container::Error;

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
            let view = resolve_clone!(ILength + IRead);
            let target: &resolve_type!(dyn IRead + ILength) = &view;
            target.length()
        }: usize as IObserved);
        register_value!(String::from("group"): String as dyn IRead + ILength);
    }.build::<Error>()?;

    assert_eq!(container.resolve_i_observed(), 5);
    let concrete = container.resolve_i_length_i_read_clone();
    let dynamic: &dyn IRead = &concrete;
    assert_eq!(dynamic.text(), "group");
    assert_eq!(concrete, "group");
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

Each container definition generates its own `SystasisContainer`. Separate modules
keep those type names distinct. In this example, `main` builds the database
container and `application::run` builds a container that uses it.

```rust
trait IDatabase {}
impl IDatabase for String {}

mod application {
    trait ILength {}
    impl ILength for usize {}

    // This component receives only the database scope.
    fn database_length(database: &primary::SubContainer<'_>) -> usize {
        database.resolve_i_database_clone().len()
    }

    #[systasis::container]
    pub fn run(primary: &super::SystasisContainer) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &super::SystasisContainer);
            register_type_with!(usize as ILength, || {
                resolve_clone_from!(IDatabase, primary).len()
            });
        }.build();

        assert_eq!(database_length(container.primary()), 11);
        assert_eq!(container.resolve_i_length(), 11);
    }
}

#[systasis::container]
fn main() {
    let Ok(database) = systasis::systasis_container! {
        register_value!(String::from("application"): String as IDatabase);
    }.build();
    application::run(&database);
}
```

`container.primary()` returns `&primary::SubContainer<'_>`, a nameable generated
scope exposing the child's applicable resolvers. No `IDatabase` trait import is
needed inside `application` for its registration-name query.

Inside a registration, nested paths use
`resolve_clone_from!(IDatabase, branch::primary)`; outside, use
`container.branch().primary().resolve_i_database_clone()`. Two instances of one
child type can be composed under different names. Child owners must remain alive
while their composed scopes are used. Ownership transfers through any path use
the same slot; composing a child does not duplicate its values.

## Thread-safety choices

Copy and Clone-only fields use immutable storage without locks or borrow counters.
Move-only fields use checked, nonblocking ownership transfer. Concurrent transfer
attempts may return `Error::ValueAccessContention`; subsequent attempts after
consumption return `Error::ValueAlreadyConsumed`. No resolver lends a stored field.

Natural `Send`/`Sync` implementations depend on stored state. Optional
`#[systasis::container(require(Send, Sync))]` checks both traits; either may be
requested independently. Positive requirements do not select a storage mode.

`#[systasis::container(require(!Sync))]` selects non-atomic `RefCell` occupancy
tracking for move-only values and makes the container `!Sync`. This does not
change the type or auto traits of the returned value. Other move-only slots use
`parking_lot` in std builds, or Spin in no_std builds. Locks are released before
returning ownership; a resolved value does not hold a container lock across
`.await`. Futures still follow ordinary Rust Send rules for everything they retain.

Returned constructor values may borrow captures. Storing a service that borrows
another registration inside the same container remains deferred.

## Features

For the no_std runtime, disable systasis's default features. With a local checkout
at `../systasis`, the application's `Cargo.toml` dependency is:

```toml
[dependencies]
systasis = { path = "../systasis", default-features = false }
```

Adjust the path to the checkout location. Procedural macros still build for the
host, and the nightly requirement above still applies.

- `std` is enabled by default. Disable default features for the no_std runtime;
  no allocator is required by systasis itself.
- `portable-atomic` enables Spin's portable atomics and critical-section
  fallback. The application supplies any required platform critical-section
  implementation; systasis does not assume a single core.
- `experimental-hardware` enables that integration and its embedded compile/link
  checks. Execution on physical boards remains unverified.
- `resolve_unchecked` enables unsafe nonblocking ownership transfer for move-only
  storage. Callers must guarantee availability and successful acquisition at the
  call. Checked accessors remain unchanged.

With `resolve_unchecked`, move-only registrations gain
`resolve_i_value_unchecked() -> T`, without `Result`. Calling it requires an
explicit unsafe context. Synchronization remains enabled; no borrowed unchecked
accessors are generated. Copy, Clone-only and fresh registrations gain no unchecked
accessor.
