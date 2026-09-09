//! The complete Quick Example from the requirements draft.
//!
//! Only lint controls are added: the source intentionally shows a logger field
//! without calling it, and spells out Default rather than deriving it.
#![forbid(unsafe_code)]
#![allow(dead_code, clippy::derivable_impls)]

use std::path::PathBuf;
use systasis::systasis_container;

trait ILogger {
    fn log(&self, msg: &str);
}

trait IConfig {
    fn path(&self) -> &PathBuf;
}

struct TracingLogger;

impl Default for TracingLogger {
    fn default() -> Self {
        Self
    }
}

impl ILogger for TracingLogger {
    fn log(&self, msg: &str) {
        println!("{}", msg);
    }
}

struct AppConfig<TLogger: ILogger> {
    logger: TLogger,
    path: PathBuf,
}

impl<TLogger: ILogger> AppConfig<TLogger> {
    pub fn new(logger: TLogger, path: PathBuf) -> Self {
        Self { logger, path }
    }
}

impl<TLogger: ILogger> IConfig for AppConfig<TLogger> {
    fn path(&self) -> &PathBuf {
        &self.path
    }
}

#[systasis::container]
fn main() {
    let Ok(container) = systasis_container! {
        // Dependencies need not precede their users in source order.
        register_type!(TracingLogger as ILogger);

        // Resolution during registration. Resolves `AppConfig`'s generic type
        // parameter to `TracingLogger` in this example
        register_type_with!(AppConfig<resolve_type!(ILogger)> as IConfig, || {
            AppConfig::new(
                // Resolves to a `Default` instance of `TracingLogger` in this example
                resolve!(ILogger),
                PathBuf::from("/etc/app/config.toml"),
            )
        });
    }.build();

    let config = container.resolve_i_config();
    println!("Config path: {:?}", config.path());
}
