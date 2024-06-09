//! Mock example
//!
//! Mocking an actor and setting it on the `SystemRegistry`. This can be
//! done so you can test an actor that depends on a `SystemService` in isolation.
#[cfg(test)]
use actix::actors::mocker::Mocker;
use actix::prelude::*;
#[cfg(test)]
use actix::SystemRegistry;

#[derive(Default)]
struct MyActor {}

impl Actor for MyActor {
    type Context = Context<Self>;
}

impl Handler<Question> for MyActor {
    type Result = ResponseFuture<usize>;

    fn handle(&mut self, _msg: Question, _ctx: &mut Context<Self>) -> Self::Result {
        let act_addr = AnswerActor::from_registry();
        let request = act_addr.send(Question {});
        Box::pin(async move { request.await.unwrap() })
    }
}

#[derive(Default)]
struct AnsActor {}

#[derive(Message)]
#[rtype(usize)]
struct Question {}

#[cfg(not(test))]
type AnswerActor = AnsActor;

#[cfg(test)]
type AnswerActor = Mocker<AnsActor>;

impl Actor for AnsActor {
    type Context = Context<Self>;
}

impl Supervised for AnsActor {}

impl SystemService for AnsActor {}

impl Handler<Question> for AnsActor {
    type Result = usize;

    fn handle(&mut self, _: Question, _: &mut Context<Self>) -> Self::Result {
        42
    }
}

#[actix::main]
async fn main() {
    let addr = MyActor::default().start();
    let result = addr.send(Question {}).await.unwrap_or_default();
    assert_eq!(42, result);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[actix::test]
    async fn mocked_behavior() {
        let mocker_addr = AnswerActor::mock(Box::new(move |_msg, _ctx| {
            let result: usize = 2;
            Box::new(Some(result))
        }))
        .start();
        SystemRegistry::set(mocker_addr);
        let result = MyActor::default().start().send(Question {}).await.unwrap();
        assert_eq!(result, 2);
    }
}
