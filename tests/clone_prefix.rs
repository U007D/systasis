//! Clone-prefixed resolvers preserve value policy across namespaces and children.
#![forbid(unsafe_code)]

use std::sync::mpsc;

#[derive(Debug, PartialEq)]
struct Message(u8);

mod channels {
    use super::*;

    trait IMessage {}
    impl IMessage for Message {}
    trait IMessageSender {}
    impl<T> IMessageSender for mpsc::Sender<T> {}
    trait IMessageReceiver {}
    impl<T> IMessageReceiver for mpsc::Receiver<T> {}

    #[systasis::container]
    pub(super) fn init() -> SystasisContainer {
        let (tx, rx) = mpsc::channel();
        let Ok(container) = systasis::systasis_container! {
            register_type!(Message as IMessage);
            register_value!(tx: mpsc::Sender<resolve_type!(IMessage)> as IMessageSender);
            register_value!(rx: mpsc::Receiver<resolve_type!(IMessage)> as IMessageReceiver);
        }
        .build();
        container
    }
}

#[test]
fn sender_resolves_by_clone_without_clone_or_default_on_message() {
    let container = channels::init();
    let direct = container.resolve_clone_i_message_sender();
    let Ok(tried) = container.try_resolve_clone_i_message_sender();
    let explicit_default = container.resolve_clone_i_message_sender_in_default();
    let Ok(tried_default) = container.try_resolve_clone_i_message_sender_in_default();
    let receiver = container.try_resolve_i_message_receiver().unwrap();

    for (value, sender) in [direct, tried, explicit_default, tried_default]
        .into_iter()
        .enumerate()
    {
        sender.send(Message(value as u8)).unwrap();
        assert_eq!(receiver.recv().unwrap(), Message(value as u8));
    }
}

mod grouped {
    trait IRead {}
    trait IWrite {}
    impl IRead for String {}
    impl IWrite for String {}

    systasis::systasis_container! {
        register_value!(String::from("group"): String as IWrite + IRead in named);
    }

    #[test]
    fn clone_prefix_precedes_sorted_group_with_namespace_suffix() {
        let Ok(container) = SystasisContainer::build();
        assert_eq!(container.resolve_clone_i_read_i_write_in_named(), "group");
        let Ok(value) = container.try_resolve_clone_i_read_i_write_in_named();
        assert_eq!(value, "group");
    }
}

mod composed {
    use super::*;

    trait IForwarded {}
    impl<T> IForwarded for mpsc::Sender<T> {}

    #[systasis::container]
    fn check(primary: &channels::SystasisContainer) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &channels::SystasisContainer);
            register_value!(
                resolve_clone_from!(IMessageSender, primary):
                resolve_type_from!(IMessageSender, primary) as IForwarded
            );
        }
        .build();

        let direct = container.primary().resolve_clone_i_message_sender();
        let Ok(tried) = container.primary().try_resolve_clone_i_message_sender();
        let forwarded = container.resolve_clone_i_forwarded();
        let receiver = primary.try_resolve_i_message_receiver().unwrap();
        for (value, sender) in [direct, tried, forwarded].into_iter().enumerate() {
            sender.send(Message(value as u8)).unwrap();
            assert_eq!(receiver.recv().unwrap(), Message(value as u8));
        }
    }

    #[test]
    fn child_methods_and_registration_queries_keep_the_same_clone_policy() {
        check(&channels::init());
    }
}
