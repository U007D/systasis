# Checked storage safety

`ReadGuard` and `WriteGuard` are systasis lock-guard wrappers. They are created only after
nonblocking lock acquisition and a successful occupancy check. They never defer
an empty-slot error until dereferencing. Taking a value requires exclusive
acquisition and leaves the slot empty. All acquisition failures are returned.

## Persistent shared reservations

The internal `TakeSlot::try_reserve_ref` operation returns an ordinary `&T`
after setting a permanent shared-access flag under the slot's write lock. It
releases that lock before returning; no native guard or allocation is retained.
The reference remains bounded by the actual borrow of the slot. This operation
does not extend lifetimes or implement pinned-container construction.

Every ownership or mutable acquisition checks the flag before forming any
mutable reference to `Option<T>` or its payload. A reserved slot returns
`ValueAccessContention` for those operations through every alias. Ordinary
shared guards, projections and cloning remain available. Repeated reservations
also require successful nonblocking exclusive acquisition; an ordinary live
read guard can prevent establishing another reservation.

The flag cannot be cleared. Even after all returned references' last uses,
mutation and taking stay contended until the slot is destroyed. Destruction
does not acquire the lock; ordinary borrow checking prevents destroying the
slot while safe caller code can still use a returned reference. Generated
internal-reference lifetimes and dependency-safe destruction need separate
verification. Temporary build reads must not automatically use this permanent
operation. The user accepted the reservation mechanism and duration on 2026-09-08.

## Standard-library backend

Each consumable slot contains `RwLock<bool>` and `UnsafeCell<Option<T>>`. The std
lock guards remain intact inside the public wrappers; systasis does not inspect
their layout or implement platform unlock operations. The slot cannot move or
be destroyed while safe code retains a usable guard borrowing it.

The checked backend has seven unsafe blocks and three unsafe `Sync` implementations:

| Site | Obligation |
| --- | --- |
| Ownership access | Hold the write lock and check no reservation exists before forming `&mut Option<T>` and taking T. |
| Shared acquisition | Hold a read lock while checking occupancy and forming `&T`. |
| Mutable acquisition | Hold the write lock and check no reservation exists before forming `&mut Option<T>` or `&mut T`. |
| Persistent reservation | Hold the write lock, form only shared payload references, set the irreversible flag before releasing the lock. |
| `ReadGuard::deref` | Its pointer targets the checked value or a valid projection, protected by the retained read lock. |
| `WriteGuard::deref` | Its pointer is valid under the retained write lock; `&self` permits only a shared reborrow. |
| `WriteGuard::deref_mut` | `&mut self` permits an exclusive reborrow under that same write lock. |
| `TakeSlot<T>: Sync` | T is `Send + Sync`: taking can transfer ownership and readers can share references. |
| `ReadGuard<T>: Sync` | T is `Sync`; sharing the wrapper exposes only shared references. |
| `WriteGuard<T>: Sync` | T is `Sync`; shared wrapper references expose only shared references, while mutation requires an exclusive wrapper reference. |

Pointers, lock guards and lifetime markers are private. `ReadGuard::map` consumes the
source wrapper, invokes a lifetime-preserving shared-reference projection, and
retains its lock. The projected target may be unsized. A caller projection panic
drops the original guard through ordinary unwinding. `WriteGuard` uses
`PhantomData<&mut T>` to preserve invariance in T. Neither std wrapper implements
`Send`; its actual std guard enforces the owning-thread drop requirement.

Forgetting a guard leaks an acquisition but cannot provide unguarded access.
Incompatible operations continue to report contention. Leaking a reference is
not a means of subsequently using it after the slot has been destroyed.

Caller unwinding through a write guard poisons the std lock. Error conversion
releases the acquired poisoned guard before returning `PoisonError<()>`; keeping
the error cannot keep the value locked or recover its contents. Poison is not
cleared. Read-guard unwinding does not poison the lock. Systasis does not catch or
recover from arbitrary caller panics, including constructors, Clone or Drop.

## no_std backend

Spin now uses the same split `RwLock<bool>` / `UnsafeCell<Option<T>>` layout.
Keeping a mapped lock over the optional payload would permit a mutable payload
reference during write acquisition, before checking the reservation; that could
invalidate an outstanding ordinary reference even if no value was changed.

Four unsafe blocks create payload references after their required checks:
owned acquisition, shared acquisition, mutable acquisition and reservation.
One unsafe `Sync` implementation requires `T: Send + Sync`, for the same reasons
as the std backend. No unsafe lifetime extension or manual destructor is used.

Spin wrappers retain the original bool lock guard and an ordinary `&T` or
`&mut T`. Dereference and shared projection are safe reference operations.
References preserve lifetimes and mutable invariance, and auto traits follow
the fields: shared guards need `T: Sync` to be Send; mutable guards need T: Send.
It uses only try-lock operations: there are no systasis acquisition retry loops
or blocking waits. The default std backend retains its native !Send guards.

Spin's accepted outstanding-reader limit remains a usage constraint. Tests on
the host do not establish embedded interrupt or multicore integration behavior.
Physical-board validation remains pending behind the planned temporary hardware
feature.

## Verification

`tests/storage.rs` exercises occupancy, contention, projection, mutation,
destruction, caller panic effects, thread sharing and guard movement. Std-only
compile-fail documentation checks guard `!Send` and mutable invariance. Miri is
required for this port because it introduces the accepted unsafe code into the
production package. The reservation change also triggers Miri on both backends;
record its actual result separately from this invariant
review. Passing Miri does not prove correctness for every program or schedule.
