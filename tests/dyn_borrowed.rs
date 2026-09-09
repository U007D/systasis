//! Dyn access must preserve non-static data in a registered implementation.
#![forbid(unsafe_code)]

trait ILogger {
    fn text(&self) -> &str;
}
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
-> Result<(), systasis::app_container::Error> {
    let input = String::from("borrowed");
    let container = systasis::systasis_container! {
        register_value!(Logger(&input): Logger<'_> as dyn ILogger);
        register_value!({
            let guard = try_resolve_dyn_ref!(ILogger)?;
            let logger: &resolve_type!(dyn ILogger) = &*guard;
            logger.text().len()
        }: usize as ILength);
    }
    .build::<systasis::app_container::Error>()?;
    assert_eq!(container.resolve_i_length(), 8);
    assert_eq!(container.try_resolve_i_logger_dyn_ref()?.text(), input);
    Ok(())
}
