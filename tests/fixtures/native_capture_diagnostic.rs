#![forbid(unsafe_code)]

trait IMessage {}
impl IMessage for String {}

#[cfg(ignored)]
macro_rules! format {
    ($literal:literal) => { String::from("configuration") };
}

#[cfg(helper)]
fn render(config: &str) -> String {
    format!("{config}")
}

#[cfg(not(any(parameter, named, generic)))]
#[systasis::container(require(Send, Sync))]
fn main() {
    let text: String = String::from("configuration");
    let unrelated: String = String::from("not captured");
    let _unused: &str = &unrelated;
    #[cfg(any(borrowed, helper, ignored))]
    let config: &str = &text;
    #[cfg(inferred)]
    let config = &text;
    #[cfg(owned)]
    let config: String = text;
    #[cfg(static_reference)]
    let config: &'static str = "configuration";

    #[cfg(ignored)]
    assert_eq!(config, "configuration");
    #[cfg(not(any(helper, inferred, wrong_output)))]
    let Ok(container) = systasis::systasis_container! {
        register_type_with!(String as IMessage, move || format!("{config}"));
    }.build();
    #[cfg(helper)]
    let Ok(container) = systasis::systasis_container! {
        register_type_with!(String as IMessage, move || render(config));
    }.build();
    #[cfg(inferred)]
    let Ok(container) = systasis::systasis_container! {
        register_type_with!(String as IMessage, move || config.to_string());
    }.build();
    #[cfg(wrong_output)]
    let Ok(container) = systasis::systasis_container! {
        register_type_with!(String as IMessage, move || { assert!(true); 7_u32 });
    }.build();

    drop(unrelated);
    #[cfg(any(ignored, static_reference, wrong_output))]
    drop(text);
    fn receive(container: &AppContainer) -> String {
        container.resolve_i_message()
    }
    assert_eq!(receive(container), "configuration");
    assert_eq!(receive(container), "configuration");
    #[cfg(helper)]
    assert_eq!(text, "configuration");
}

#[cfg(parameter)]
#[systasis::container]
fn check(config: &str) {
    let Ok(container) = systasis::systasis_container! {
        register_type_with!(String as IMessage, move || format!("{config}"));
    }.build();
    assert_eq!(container.resolve_i_message(), "configuration");
}

#[cfg(named)]
#[systasis::container(require(Send, Sync))]
fn check<'env>(config: &'env str) {
    let Ok(container) = systasis::systasis_container! {
        register_type_with!(String as IMessage, move || format!("{config}"));
    }.build();
    assert_eq!(container.resolve_i_message(), "configuration");
}

#[cfg(generic)]
#[systasis::container(require(Send, Sync))]
fn check<T: AsRef<str> + Send + Sync>(config: T) {
    let Ok(container) = systasis::systasis_container! {
        register_type_with!(String as IMessage, move || format!("{}", config.as_ref()));
    }.build();
    assert_eq!(container.resolve_i_message(), "configuration");
}

#[cfg(any(parameter, named, generic))]
fn main() {
    let text: String = String::from("configuration");
    check(&text);
    assert_eq!(text, "configuration");
}
