//! An explicitly Sync dyn target preserves the read guard's Send behavior.
#![forbid(unsafe_code)]

trait ILogger {
    fn text(&self) -> &str;
}
impl ILogger for String {
    fn text(&self) -> &str {
        self
    }
}

#[systasis::container(require(Send, Sync))]
#[test]
fn sync_combined_target_can_be_read_on_another_scoped_thread() {
    let Ok(container) = systasis::systasis_container! {
        register_value!(String::from("threaded"): String as dyn ILogger + core::marker::Sync);
    }
    .build();
    let guard = container.try_resolve_i_logger_sync_dyn_ref().unwrap();
    std::thread::scope(|scope| {
        scope
            .spawn(move || assert_eq!(guard.text(), "threaded"))
            .join()
            .unwrap();
    });
    assert_eq!(container.try_resolve_i_logger_sync().unwrap(), "threaded");
}
