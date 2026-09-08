# Checked storage safety

`Ref` and `RefMut` are systasis lock-guard wrappers. They are created only after
nonblocking lock acquisition and a successful occupancy check. They never defer
an empty-slot error until dereferencing. Taking a value requires exclusive
acquisition and leaves the slot empty. All acquisition failures are returned.

## Standard-library backend

Each consumable slot contains `RwLock<()>` and `UnsafeCell<Option<T>>`. The std
lock guards remain intact inside the public wrappers; systasis does not inspect
their layout or implement platform unlock operations. The slot cannot move or
be destroyed while safe code retains a usable guard borrowing it.

The checked backend has six unsafe blocks and three unsafe `Sync` implementations:

| Site | Obligation |
| --- | --- |
| Ownership access | Hold the write lock while forming `&mut Option<T>` and taking T. |
| Shared acquisition | Hold a read lock while checking occupancy and forming `&T`. |
| Mutable acquisition | Hold the write lock while checking occupancy and forming `&mut T`. |
| `Ref::deref` | Its pointer targets the checked value or a valid projection, protected by the retained read lock. |
| `RefMut::deref` | Its pointer is valid under the retained write lock; `&self` permits only a shared reborrow. |
| `RefMut::deref_mut` | `&mut self` permits an exclusive reborrow under that same write lock. |
| `TakeSlot<T>: Sync` | T is `Send + Sync`: taking can transfer ownership and readers can share references. |
| `Ref<T>: Sync` | T is `Sync`; sharing the wrapper exposes only shared references. |
| `RefMut<T>: Sync` | T is `Sync`; shared wrapper references expose only shared references, while mutation requires an exclusive wrapper reference. |

Pointers, lock guards and lifetime markers are private. `Ref::map` consumes the
source wrapper, invokes a lifetime-preserving shared-reference projection, and
retains its lock. The projected target may be unsized. A caller projection panic
drops the original guard through ordinary unwinding. `RefMut` uses
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

Spin's `lock_api` mapped guards protect `RwLock<Option<T>>`. Safe `try_map` checks
occupancy while retaining acquisition; mapping failure releases the guard. This
backend's systasis implementation forbids unsafe code. It uses only try-lock
operations: there are no systasis acquisition retry loops or blocking waits.

Spin's accepted outstanding-reader limit remains a usage constraint. Tests on
the host do not establish embedded interrupt or multicore integration behavior.
Physical-board validation remains pending behind the planned temporary hardware
feature.

## Verification

`tests/storage.rs` exercises occupancy, contention, projection, mutation,
destruction, caller panic effects, thread sharing and guard movement. Std-only
compile-fail documentation checks guard `!Send` and mutable invariance. Miri is
required for this port because it introduces the accepted unsafe code into the
production package; record its actual result separately from this invariant
review. Passing Miri does not prove correctness for every program or schedule.
