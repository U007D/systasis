# Toolchain migration

The `stable-impl` branch now supports stable Rust 1.100 and newer. See
[stable support and API migration](STABLE.md) for the current requirements,
generic container signatures, and verification commands.

Earlier revisions used `type_alias_impl_trait` and `allow_internal_unstable`.
Those features have been removed from this branch's implementation. Historical
validation entries describe the compiler and revision used at that time.
