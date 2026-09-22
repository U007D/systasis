//! Child scope signatures do not inherit unrelated parent generic parameters.
#![forbid(unsafe_code)]

trait Related<U> {
    fn relate(&self, other: &U);
}
impl Related<String> for u32 {
    fn relate(&self, other: &String) {
        assert_eq!(*self as usize, other.len());
    }
}

mod child {
    pub trait IValue {}
    impl<T> IValue for T {}
    #[systasis::container]
    pub fn run<T: Copy>(value: T, use_child: impl FnOnce(&SystasisContainer<T>)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: T as IValue);
        }
        .build();
        use_child(&container);
    }
}

mod outer {
    use super::Related;
    type Alias<T> = super::child::SystasisContainer<T>;
    fn lookup<T: Copy>(scope: &primary::SubContainer<'_, T>) -> T {
        scope.resolve_i_value()
    }
    #[systasis::container]
    pub fn run<T: Copy + Related<U>, U>(primary: &Alias<T>, other: U) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &Alias<T>);
        }
        .build();
        lookup(container.primary()).relate(&other);
    }
}

mod outer_where {
    use super::Related;
    type Alias<T> = super::child::SystasisContainer<T>;
    fn lookup<T: Copy>(scope: &primary::SubContainer<'_, T>) -> T {
        scope.resolve_i_value()
    }
    #[systasis::container]
    pub fn run<T, U>(primary: &Alias<T>, other: U)
    where
        T: Copy + Related<U>,
    {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &Alias<T>);
        }
        .build();
        lookup(container.primary()).relate(&other);
    }
}

#[test]
fn child_alias_keeps_copy_but_not_parent_only_relationship() {
    child::run(4_u32, |child| outer::run(child, String::from("four")));
    child::run(4_u32, |child| outer_where::run(child, String::from("four")));
}
