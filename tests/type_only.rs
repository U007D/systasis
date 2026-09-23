//! Constructor-free type lookup and construction when a constructor is available.
#![forbid(unsafe_code)]

mod channels {
    use std::sync::mpsc;

    #[derive(Debug, PartialEq)]
    struct Message(u8);
    trait IMessage {}
    impl IMessage for Message {}
    trait IMessageSender {}
    impl<T> IMessageSender for mpsc::Sender<T> {}
    trait IMessageReceiver {}
    impl<T> IMessageReceiver for mpsc::Receiver<T> {}

    #[systasis::container]
    fn init() -> SystasisContainer {
        let (tx, rx) = mpsc::channel();
        let builder = systasis::systasis_container! {
            register_type!(Message as IMessage);
            register_value!(rx: mpsc::Receiver<resolve_type!(IMessage)> as IMessageReceiver);
            register_value!(tx: mpsc::Sender<resolve_type!(IMessage)> as IMessageSender);
        };
        let Ok(container) = builder.build();
        container
    }

    #[test]
    fn non_default_message_types_configure_channel_endpoints() {
        let container = init();
        let tx = container.resolve_i_message_sender_clone();
        let rx = container.try_resolve_i_message_receiver().unwrap();
        tx.send(Message(42)).unwrap();
        assert_eq!(rx.recv().unwrap(), Message(42));
    }
}

mod declared {
    use core::marker::PhantomData;

    pub struct Message;
    trait IMessage {}
    impl IMessage for Message {}

    systasis::systasis_container! {
        register_type!(Message as IMessage);
        register_value!(PhantomData: PhantomData<registered_type!(IMessage)> as Copy);
    }

    #[test]
    fn declaration_type_lookup_needs_no_constructor() {
        let Ok(container) = SystasisContainer::build();
        let _: PhantomData<Message> = container.resolve_copy();
    }
}

mod generic {
    trait IMessage {}
    struct Message;
    impl IMessage for Message {}
    trait IValues {}
    impl<T> IValues for Vec<T> {}

    #[systasis::container]
    fn init<T: IMessage>() -> SystasisContainer<T> {
        let builder = systasis::systasis_container! {
            register_type!(T as IMessage);
            register_value!(Vec::new(): Vec<resolve_type!(IMessage)> as IValues);
        };
        let Ok(container) = builder.build();
        container
    }

    #[test]
    fn a_generic_type_query_requires_no_default_bound() {
        let container = init::<Message>();
        let values: Vec<Message> = container.try_resolve_i_values().unwrap();
        assert!(values.is_empty());
    }
}

#[allow(
    clippy::type_complexity,
    reason = "child type queries expand into generated scope metadata projections"
)]
mod composition {
    use super::declared;
    use core::marker::PhantomData;

    #[systasis::container]
    fn verify(primary: &declared::SystasisContainer) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &declared::SystasisContainer);
            register_value!(PhantomData: PhantomData<resolve_type_from!(IMessage, primary)> as Copy);
        }.build();
        let _: PhantomData<declared::Message> = container.resolve_copy();
    }

    #[test]
    fn a_child_type_mapping_does_not_require_child_value_resolution() {
        let Ok(child) = declared::SystasisContainer::build();
        verify(&child);
    }
}

mod constructors {
    use core::sync::atomic::{AtomicUsize, Ordering};

    static DEFAULT_CALLS: AtomicUsize = AtomicUsize::new(0);
    static CUSTOM_CALLS: AtomicUsize = AtomicUsize::new(0);

    struct Message;
    trait IMessage {}
    impl IMessage for Message {}
    impl Default for Message {
        fn default() -> Self {
            DEFAULT_CALLS.fetch_add(1, Ordering::SeqCst);
            Self
        }
    }

    struct CustomMessage;
    trait ICustomMessage {}
    impl ICustomMessage for CustomMessage {}

    systasis::systasis_container! {
        register_type!(Message as IMessage);
        register_type_with!(CustomMessage as ICustomMessage, || {
            CUSTOM_CALLS.fetch_add(1, Ordering::SeqCst);
            CustomMessage
        });
    }

    #[test]
    fn default_and_custom_constructors_stay_lazy_and_repeatable() {
        let Ok(container) = SystasisContainer::build();
        assert_eq!(DEFAULT_CALLS.load(Ordering::SeqCst), 0);
        assert_eq!(CUSTOM_CALLS.load(Ordering::SeqCst), 0);
        for _ in 0..2 {
            let _: Message = container.resolve_i_message();
            let _: CustomMessage = container.resolve_i_custom_message();
        }
        assert_eq!(DEFAULT_CALLS.load(Ordering::SeqCst), 2);
        assert_eq!(CUSTOM_CALLS.load(Ordering::SeqCst), 2);
    }
}
