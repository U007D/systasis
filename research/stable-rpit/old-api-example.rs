//! Current Systasis API; compile with nightly and the production crate.
#![forbid(unsafe_code)]

use systasis::systasis_container;

trait INumber {}
impl INumber for u8 {}

systasis_container! {
    register_type_with!(u8 as INumber, || 42u8);
}

fn init() -> Result<SystasisContainer, SystasisContainerError> {
    SystasisContainer::build()
}

fn read_number(container: &SystasisContainer) -> u8 {
    container.resolve_i_number()
}

fn main() {
    let Ok(container) = init();
    println!("{}", read_number(&container));
}
