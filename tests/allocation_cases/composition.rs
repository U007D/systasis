//! Includes independently owned leaf, middle, and outer containers in each count.

use super::{assert_no_allocations, measure};
use systasis::container::Error;

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
                pub fn run(value: u32, call: impl FnOnce(&SystasisContainer) -> Result<(), Error>) -> Result<(), Error> {
                    let Ok(container) = systasis::systasis_container! {
                        register_value!(Packet([value; 4]): Packet as dyn IPacket);
                    }.build();
                    call(&container)
                }
            }

            mod middle {
                use super::*;
                #[systasis::container($($requirements)*)]
                pub fn run<'a>(primary: &'a leaf::SystasisContainer, replica: &'a leaf::SystasisContainer, call: impl FnOnce(&SystasisContainer<'a>) -> Result<(), Error>) -> Result<(), Error> {
                    let Ok(container) = systasis::systasis_container! {
                        register_container!(primary: &'a leaf::SystasisContainer);
                        register_container!(replica: &'a leaf::SystasisContainer);
                    }.build();
                    call(&container)
                }
            }

            mod outer {
                use super::*;
                trait ICopied {}
                impl ICopied for leaf::Packet {}
                trait IObserved {}
                impl IObserved for u32 {}
                #[systasis::container($($requirements)*)]
                pub fn run<'a>(branch: &middle::SystasisContainer<'a>) -> Result<(), Error> {
                    let container = systasis::systasis_container! {
                        register_container!(branch: &middle::SystasisContainer<'a>);
                        register_value!(resolve_clone_from!(IPacket, branch::primary): resolve_type_from!(IPacket, branch::primary) as ICopied);
                        register_value!({
                            let packet = resolve_clone_from!(IPacket, branch::primary);
                            let object: &resolve_type_from!(dyn IPacket, branch::primary) = &packet;
                            object.first()
                        }: u32 as IObserved);
                    }.build::<Error>()?;
                    assert_eq!(container.resolve_i_observed(), 11);
                    let mut packet = container.branch().primary().resolve_i_packet_clone();
                    assert_eq!(packet.0, [11; 4]);
                    packet.0[0] = 12;
                    let object: &dyn leaf::IPacket = &packet;
                    assert_eq!(object.first(), 12);
                    let Ok(another) = container.branch().primary().try_resolve_i_packet_clone();
                    assert_eq!(another.0, [11; 4]);
                    drop((packet, another));
                    let Ok(cloned) = container.try_resolve_i_copied_clone();
                    assert_eq!(cloned.0, [11; 4]);
                    drop(cloned);
                    // Both copied storage and the untouched replica drop with their owners.
                    core::hint::black_box(container);
                    Ok(())
                }
            }

            mod native_outer {
                use super::*;
                struct View(leaf::Packet);
                trait IView {}
                impl IView for View {}

                #[systasis::container($($requirements)*)]
                pub fn run<'a>(branch: &middle::SystasisContainer<'a>) -> Result<(), Error> {
                    let container = systasis::systasis_container! {
                        register_container!(branch: &middle::SystasisContainer<'a>);
                        register_type_with!(View as IView, try || -> Result<View, Error> {
                            let packet = resolve_clone_from!(IPacket, branch::primary);
                            // This macro selects native constructor storage.
                            assert_eq!(packet.0[0], 11);
                            Ok(View(packet))
                        });
                    }.build::<Error>()?;
                    for _ in 0..2 {
                        let mut view = core::hint::black_box(&container).try_resolve_i_view()?;
                        assert_eq!(view.0.0, [11; 4]);
                        view.0.0[0] = 12;
                        assert_eq!(branch.primary().resolve_i_packet_clone().0, [11; 4]);
                        drop(view);
                    }
                    Ok(())
                }
            }

            #[test]
            fn native_nested_child_contexts_and_cloned_outputs_do_not_allocate() {
                let (result, counts) = measure(|| {
                    leaf::run(11, |primary| {
                        leaf::run(22, |replica| {
                            middle::run(primary, replica, native_outer::run)
                        })
                    })
                });
                result.unwrap();
                assert_no_allocations(counts);
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
