//! Includes independently owned leaf, middle, and outer containers in each count.

use super::{assert_no_allocations, measure};
use systasis::app_container::Error;

macro_rules! scenario {
    ($module:ident, ($($requirements:tt)*)) => {
        mod $module {
            use super::*;

            mod leaf {
                use super::Error;
                pub trait IPacket { fn first(&self) -> u32; }
                #[derive(Clone)]
                pub struct Packet(pub [u32; 4]);
                impl IPacket for Packet { fn first(&self) -> u32 { self.0[0] } }
                impl Drop for Packet {
                    fn drop(&mut self) { core::hint::black_box(&self.0); }
                }
                #[systasis::container($($requirements)*)]
                pub fn run(value: u32, call: impl FnOnce(&AppContainer) -> Result<(), Error>) -> Result<(), Error> {
                    let Ok(container) = systasis::systasis_container! {
                        register_value!(Packet([value; 4]): Packet as dyn IPacket);
                    }.build();
                    call(container)
                }
            }

            mod middle {
                use super::*;
                #[systasis::container($($requirements)*)]
                pub fn run<'a>(primary: &'a leaf::AppContainer, replica: &'a leaf::AppContainer, call: impl FnOnce(&AppContainer<'a>) -> Result<(), Error>) -> Result<(), Error> {
                    let Ok(container) = systasis::systasis_container! {
                        register_container!(primary: &'a leaf::AppContainer);
                        register_container!(replica: &'a leaf::AppContainer);
                    }.build();
                    call(container)
                }
            }

            mod outer {
                use super::*;
                trait ICopied {}
                impl ICopied for leaf::Packet {}
                trait IObserved {}
                impl IObserved for u32 {}
                #[systasis::container($($requirements)*)]
                pub fn run<'a>(branch: &middle::AppContainer<'a>) -> Result<(), Error> {
                    let container = systasis::systasis_container! {
                        register_container!(branch: &middle::AppContainer<'a>);
                        register_value!(try_resolve_clone_from!(IPacket, branch::primary)?: resolve_type_from!(IPacket, branch::primary) as ICopied);
                        register_value!({
                            let guard = try_resolve_dyn_ref_from!(IPacket, branch::primary)?;
                            guard.first()
                        }: u32 as IObserved);
                    }.build::<Error>()?;
                    assert_eq!(container.resolve_i_observed(), 11);
                    {
                        let reader = container.branch().primary().try_resolve_i_packet_ref()?;
                        assert_eq!(reader.0, [11; 4]);
                        assert!(matches!(container.branch().primary().try_resolve_i_packet(), Err(Error::ValueAccessContention)));
                    }
                    container.branch().primary().try_resolve_i_packet_ref_mut()?.0[0] = 12;
                    assert_eq!(container.branch().primary().try_resolve_i_packet_dyn_ref()?.first(), 12);
                    let consumed = container.branch().primary().try_resolve_i_packet()?;
                    assert_eq!(consumed.0[0], 12);
                    assert!(matches!(container.branch().primary().try_resolve_i_packet_ref(), Err(Error::ValueAlreadyConsumed)));
                    drop(consumed);
                    let cloned = container.try_resolve_i_copied_clone()?;
                    assert_eq!(cloned.0, [11; 4]);
                    drop(cloned);
                    // Both copied storage and the untouched replica drop with their owners.
                    core::hint::black_box(container);
                    Ok(())
                }
            }

            #[test]
            fn nested_build_resolution_and_destruction_do_not_allocate() {
                let (result, counts) = measure(|| {
                    leaf::run(11, |primary| {
                        leaf::run(22, |replica| {
                            middle::run(primary, replica, outer::run)
                        })
                    })
                });
                result.unwrap();
                assert_no_allocations(counts);
            }
        }
    };
}

scenario!(synchronized, ());
scenario!(local, (require(!Sync)));
