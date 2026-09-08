//! Statically wired dependency injection.
//!
//! This package is under development. Container generation is not implemented yet.
#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod app_container;

mod fallible;
pub use fallible::Fallible;
