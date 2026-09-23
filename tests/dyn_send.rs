//! An explicitly Sync dyn target can borrow an owned value across scoped threads.
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
    let owned = container.resolve_clone_i_logger_sync();
    let target: &(dyn ILogger + Sync) = &owned;
    std::thread::scope(|scope| {
        scope
            .spawn(move || assert_eq!(target.text(), "threaded"))
            .join()
            .unwrap();
    });
    let Ok(another) = container.try_resolve_clone_i_logger_sync();
    assert_eq!(another, "threaded");
}
