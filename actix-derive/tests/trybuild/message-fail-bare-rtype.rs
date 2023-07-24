use actix::prelude::*;

#[derive(MessageResponse)]
struct Added(usize);

#[derive(Message)]
#[rtype(result = Added)]
struct Sum(usize, usize);

fn main() {}
