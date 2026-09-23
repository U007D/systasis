# Checked storage safety

## Generated ownership and the builder

A consuming builder retains a safe `FnOnce` initializer. Building executes it
and returns an owned `SystasisContainer`; there is no hidden scope owner or allocation.
The container owns its slots and constructor captures. It may retain references
to external inputs, with ordinary Rust lifetime checks. Stored references are
resolved by value: shared references copy, mutable references transfer once.
Neither operation lends the container's slot. Constructor outputs may borrow
captured state; those borrows prevent moving or destroying their container.
Stored internal borrows are not supported by this mechanism.

The generated type remains `!Unpin`, but construction does not pin its result.
No storage operation relies on pinning: address stability during access follows
from the resolver borrow. Abandoning the builder drops its captures without
running initialization. Failure drops partial state before returning, except
values owned by the error. The builder adds no unsafe code or invariant panics.

## Test-only allocator instrumentation

`tests/allocation_cases/counter.rs` contains the separately approved counting
allocator: one unsafe `GlobalAlloc` implementation and four unsafe calls
forwarding unchanged to `std::alloc::System`. It is linked only into the
`allocations` test binary; library unsafe code and dependencies are unchanged.
Pointers, layouts, allocation ownership and failure results are forwarded intact.
The hooks only update thread-local `Cell` counters with saturating arithmetic;
they do not format, assert, lock or unwind. Rust's
[allocator re-entrance contract](https://doc.rust-lang.org/std/alloc/trait.GlobalAlloc.html#re-entrance)
guarantees that thread-local storage does not invoke the global allocator.

Assertions run in tests, not allocator hooks. A scoped measurement clears its
state during unwinding; another thread cannot modify that thread's counters.
Positive controls exercise each of the four allocator entry points. These are
observations of allocator calls, including attempted allocations, not bytes or
live allocations. Optimization can remove allocations. No safety argument relies
on observing a particular count. See VALIDATION.md for measured workloads.

## Storage policies

- CopySlot stores a repeatably copied value directly, without synchronization.
- ReadSlot stores an immutable, untakeable value directly, without a lock or
  borrow counter. A Sync payload permits sharing this slot across threads.
- LocalTakeSlot uses RefCell<Option<T>>, with fallible borrowing and occupancy
  checks. It has no atomic synchronization and is !Sync. Returned core::cell
  guards are !Send. This is an internal storage primitive, not a new public
  resolver naming contract.
- TakeSlot supports synchronized mutable/consuming access through &self.
  std uses parking_lot 0.12.5 with send_guard; no_std uses spin.

The generator selects CopySlot for Copy values, ReadSlot for Clone-only values,
and TakeSlot/LocalTakeSlot otherwise. Clone storage is immutable and never consumed,
so cloning cannot fail through occupancy or contention. Copy and Clone try aliases
return Result<T, !>. Shared references intrinsically copy; &mut T moves once.
Generic types use declaration-site bounds; their API does not change at instantiation.

Selection applies equally through subcontainers. Absence of require(Sync) is not
proof of single-threaded use and does not disable natural Sync capability.
No generated method returns the internal slot's read or write guard; locks and
local borrow tracking are released before ownership returns to the caller.

## Synchronized slots and guards

Guard operations remain internal support with regression coverage; generated
containers now expose only ownership transfer from these slots. Their existing
unsafe implementation is unchanged by the resolver-policy change.

Both backends store RwLock<bool> separately from UnsafeCell<Option<T>>.
Four unsafe blocks form payload references only after the relevant checks:
taking, shared acquisition, mutable acquisition, and the legacy reservation API.
One unsafe Sync implementation per backend requires T: Send + Sync. Sharing
allows concurrent readers; taking can transfer ownership to another thread.

Ref and RefMut retain the acquired bool-lock guard and an ordinary &T or &mut T.
Occupancy is checked before either wrapper is returned. Dereference and shared
projection are safe reference operations, with no optional-value unwrap.
RefMut is invariant in T. Ref::map consumes the original wrapper, retaining its
lock and projecting a reference with the original reference's lifetime.
Private fields prevent manufacturing guards without checks.

Auto traits follow the fields: Ref needs T: Sync to be Send; RefMut needs T: Send.
Both need T: Sync to be Sync. parking_lot's send_guard feature permits unlocking
on a different thread. No systasis unsafe Send implementation is needed.
This changes the former std guard thread-affinity restriction, not lifetimes:
backing slots still cannot move or be destroyed while a usable guard borrows them.

Acquisition uses only try-lock methods, with no systasis retry or blocking loop.
The only checked access errors are ValueAlreadyConsumed and ValueAccessContention.
Neither backend poisons on caller panic. Unwinding releases guards normally;
subsequent operations can observe whatever value the caller left behind.
Systasis does not catch or repair caller constructors, destructors, or mutations.

## Portable atomic fallback

The explicit `portable-atomic` feature enables Spin's portable atomics with
`critical-section` support. The application supplies the platform implementation,
including interrupt exclusion, multicore exclusion and nested acquire/release
behavior. No unsafe single-core or privileged-mode assumption is selected.
Each fallback atomic operation enters and leaves its own critical section;
that critical section is not held for the lifetime of a systasis Ref/RefMut.
The ordinary Spin lock state still protects the payload while a guard exists.

Try-lock contention remains nonblocking at the container API, but a platform
critical-section implementation may itself wait. Neither bounded interrupt
latency nor hard real-time behavior follows from the feature alone. Native host
Miri tests exercise the selected dependency's native atomic path; embedded
compile/link checks do not establish correctness of an application critical
section or execution on a physical board.

## Legacy reservation operation

The internal try_reserve_ref API remains for existing research compatibility.
It is not an approved premise for the current generated-container API.
It sets an irreversible flag under exclusive locking before returning &T.
Taking/mutable access check the flag before forming any mutable payload reference.
Shared access remains permitted. This operation extends neither lifetimes nor
the address stability of storage. No new injection code calls it.

## Unsynchronized slots

LocalTakeSlot contains no unsafe code. RefCell::try_borrow/try_borrow_mut reject
conflicts; Ref::filter_map/RefMut::filter_map check occupancy and retain the borrow.
Missing values release the temporary borrow before reporting consumption.
Taking requires exclusive borrowing. ReadSlot has no mutation/taking methods.
Forgetting any guard can retain contention; it cannot enable incompatible access.

## Verification

The default-off `resolve_unchecked` feature adds an unsafe owned accessor for
generated move-only slots. It calls the same checked acquisition function,
then return the success value. Caller-precondition violations reach justified
`unreachable!` diagnostics; no `unwrap_unchecked` or additional payload pointer
operation is introduced. Generated methods forward the caller's preconditions
through explicit unsafe calls. Registration-time queries do not add an unsafe
block on the caller's behalf. Copy/Clone/fresh storage gains no unchecked accessor.
The low-level legacy borrowed primitives remain tested but are not emitted as
container accessors.

Historical guard-based feature tests passed natively and under Miri for std and
no_std, including local RefCell guards and a `forbid(unsafe_code)` consumer using
only checked access. Compiler tests reject calls outside unsafe context and
methods excluded by storage policy. The owned-only API has separate current
coverage recorded in VALIDATION.md; historical guard results are not its test count. No dependency changed.

Native Rust-driven compiler tests reject lifetime escape, mutable lifetime
substitution, sharing local storage, sending local guards, and taking ReadSlot.
Positive tests establish transferable synchronized guards, including projected
read guards, and unlocking on the receiving thread. Panic tests verify release
without poisoning. Occupancy, projections, alignment, destruction, forgotten
guards, and the legacy reservation API retain regression coverage.

Miri passes the storage, reservation, and unsynchronized tests on both backends
(31 tests per backend). Native tests and Clippy are separate checks, not a proof
of correctness for every schedule. Host no_std tests do not establish hardware
interrupt or multicore correctness. Spin's previously accepted reader-count
limit remains a usage constraint; hardware validation is still pending.
