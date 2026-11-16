# Why the prototype uses BuildResult

This investigation is parked on `stable-impl`; see [the user decision](../README.md).

The user asked whether Result and SystasisContainerError must be replaced and
whether the demonstrated API is minimal. No global minimality is established.

Neither type is replaced by a different result/error mechanism:

```rust,ignore
type BuildResult<F, E> = Result<
    SystasisContainer<F, E>,
    SystasisContainerError<E>,
>;
```

In `BuildResult<impl Fn() -> u8, impl Error>`, the second opaque argument supplies
the inferred payload E, which is still wrapped/selected by the generated error
alias. Infallible selection yields actual never. Fallible selection retains the
inferred inline source enum. The standard Error bound is not a trait object or a
replacement of the named error alias with a boxed catch-all.

The alias shares one inferred E between the container's phantom marker and its
error. Expanding it into two separate `impl Error` occurrences does not create a
shared name: the occurrences declare distinct opaque types.

## Direct Result check

`explicit-result-existing.rs`, compiled against the previous stable provider,
checks the direct spelling:

```rust,ignore
fn init() -> Result<
    SystasisContainer<impl Fn() -> u8, impl Error>,
    SystasisContainerError<impl Error>,
> {
    SystasisContainer::build()
}
```

The possibly fallible version compiles and runs on stable 1.100.0 with bootstrap
removed and unstable features forbidden. Reproduce with `--cfg
'feature="fallible_only"'`, the same --extern provider path as the other consumer
fixtures, and `-Dwarnings -Funstable-features`.

The infallible version fails E0277: its second opaque type falls back to `()`,
which does not implement Error. Selecting actual never erased that parameter's
only occurrence. The first opaque E in the container remains well defined, but
does not constrain the independent second E. The shared-parameter BuildResult
alias avoids that failure. This is why it was useful beyond shortening a name.

`minimal-selector.rs` also tries giving the infallible selector only one valid
payload witness, hoping its trait obligation would infer the opaque payload.
The stable compiler rejects this (E0277 with the explicit Infallible witness).
This does not establish that every other encoding fails.

After running the main driver as documented in README.md, reproduce the passing
explicit-Result wrapper with:

```sh
env -u RUSTC_BOOTSTRAP rustc +stable --edition=2024 -Dwarnings -Funstable-features \
  --cfg 'feature="fallible_only"' research/stable-rpit/explicit-result-existing.rs \
  --extern stable_rpit=/private/tmp/systasis-stable-rpit-results/core/libstable_rpit.rlib \
  -o /private/tmp/systasis-explicit-result-fallible
/private/tmp/systasis-explicit-result-fallible
```

Omitting `--cfg 'feature="fallible_only"'` includes the intentionally failing
infallible case. `minimal-selector.rs` is also an intentionally rejected candidate,
not an example expected to compile.

## Conclusion

The proposed change is generic public naming, not a new runtime Result type or
an erased error policy. BuildResult is the tested general signature for this
representation; it is optional in the demonstrated possibly-fallible wrapper.
Neither a global minimal API nor an unchanged concrete public error/container
name is demonstrated. Public API selection and production integration remain
unapproved; no production changes were made.
