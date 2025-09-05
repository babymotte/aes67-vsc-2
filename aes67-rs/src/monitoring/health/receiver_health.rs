use crate::receiver::config::RxDescriptor;

pub struct ReceiverHealth {
    id: String,
    desc: Option<RxDescriptor>,
}
