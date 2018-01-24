pub mod sync;
pub mod unsync;

#[cfg_attr(feature="cargo-clippy", allow(module_inception))]
mod queue;


pub enum MessageOption {
    Envelope,
    Message,
}


pub enum Either<A, B> {
    Envelope(A),
    Message(B),
}

impl<A, B> Either<A, B> {
    pub fn unwrap_envelope(self) -> A {
        match self {
            Either::Envelope(a) => a,
            Either::Message(_) => panic!(),
        }
    }

    pub fn unwrap_message(self) -> B {
        match self {
            Either::Message(b) => b,
            Either::Envelope(_) => panic!(),
        }
    }
}
