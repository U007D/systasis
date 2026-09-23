//! Explicit dyn borrows preserve non-static data in a resolved implementation.
#![forbid(unsafe_code)]

trait ILogger {
    fn text(&self) -> &str;
}
#[derive(Clone)]
struct Logger<'a>(&'a str);
impl ILogger for Logger<'_> {
    fn text(&self) -> &str {
        self.0
    }
}
trait ILength {}
impl ILength for usize {}

#[systasis::container]
#[test]
fn dyn_references_do_not_require_static_implementation_data()
-> Result<(), systasis::container::Error> {
    let input = String::from("borrowed");
    let container = systasis::systasis_container! {
        register_value!(Logger(&input): Logger<'_> as dyn ILogger);
        register_value!({
            let owned = resolve_clone!(ILogger);
            let logger: &resolve_type!(dyn ILogger) = &owned;
            logger.text().len()
        }: usize as ILength);
    }
    .build::<systasis::container::Error>()?;
    assert_eq!(container.resolve_i_length(), 8);
    let owned = container.resolve_clone_i_logger();
    let logger: &dyn ILogger = &owned;
    assert_eq!(logger.text(), input);
    Ok(())
}
