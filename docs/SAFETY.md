# Checked storage safety

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

The generator selects plain Copy storage or consumable storage using the
registration-site Copy policy. Selection must consider every access path, including independently
shared subcontainers. Absence of require(Sync) is not proof of single-threaded
use and must not silently disable the container's natural Sync capability.
Local guard types and synchronized guard types differ; generated resolver
return types must select the actual policy without erasing their auto traits.

## Synchronized slots and guards

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

The default-off `resolve_unchecked` feature adds unsafe owned/shared/mutable
accessors for consumable slots. They call the same checked acquisition functions,
then return the success value. Caller-precondition violations reach justified
`unreachable!` diagnostics; no `unwrap_unchecked` or additional payload pointer
operation is introduced. Generated methods forward the caller's preconditions
through explicit unsafe calls. Registration-time queries do not add an unsafe
block on the caller's behalf. Copy/fresh storage gains no unchecked accessor,
and constructor-borrow ownership exclusions remain in force.

Four feature-enabled behavior tests pass natively and under Miri for std and
no_std, including local RefCell guards and a `forbid(unsafe_code)` consumer using
only checked access. Compiler tests reject calls outside unsafe context and
methods excluded by storage policy or constructor borrowing. No dependency changed.

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
