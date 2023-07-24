use actix::MessageResponse;

#[derive(MessageResponse)]
struct Added(usize);

fn main() {}
