#![cfg(feature = "macros")]

use std::collections::HashSet;

use actix::prelude::*;

#[derive(Clone, Debug)]
struct Num(usize);

impl Message for Num {
    type Result = ();
}

struct SessionActor {
    sessions: HashSet<usize>,
}

impl SessionActor {
    fn new() -> Self {
        Self {
            sessions: HashSet::new(),
        }
    }

    fn session_count(&self) -> usize {
        self.sessions.len()
    }

    fn sessions(&self) -> Vec<usize> {
        self.sessions.iter().cloned().collect()
    }

    fn add_session(&mut self, id: usize) -> Result<usize, String> {
        match self.sessions.insert(id) {
            false => Err(String::from("Duplicate session ID")),
            true => Ok(id),
        }
    }
}

impl Actor for SessionActor {
    type Context = Context<Self>;
}

#[derive(Message)]
#[rtype(result = "Result<usize, String>")]
struct AddSession(usize);

impl Handler<AddSession> for SessionActor {
    type Result = Result<usize, String>;

    fn handle(&mut self, id: AddSession, _: &mut Context<Self>) -> Self::Result {
        self.add_session(id.0)
    }
}

#[derive(Message)]
#[rtype(result = "usize")]
struct GetSessionCount;

impl Handler<GetSessionCount> for SessionActor {
    type Result = usize;

    fn handle(&mut self, _: GetSessionCount, _: &mut Context<Self>) -> Self::Result {
        self.session_count()
    }
}

#[derive(Message)]
#[rtype(result = "Vec<usize>")]
struct GetConnectedSessions;

impl Handler<GetConnectedSessions> for SessionActor {
    type Result = Vec<usize>;

    fn handle(&mut self, _: GetConnectedSessions, _: &mut Context<Self>) -> Self::Result {
        self.sessions()
    }
}

#[derive(Message)]
#[rtype(result = "Option<usize>")]
struct GetSessionById(usize);

impl Handler<GetSessionById> for SessionActor {
    type Result = Option<usize>;

    fn handle(&mut self, id: GetSessionById, _: &mut Context<Self>) -> Self::Result {
        self.sessions.get(&id.0).cloned()
    }
}

#[actix::test]
async fn test_different_message_result_types() {
    let actor = SessionActor::new().start();

    let count = actor.send(GetSessionCount).await.unwrap();

    assert!(
        count == 0,
        "Invalid message response as the ActorSession's sessions should be empty by default"
    );

    actor
        .send(AddSession(1))
        .await
        .unwrap()
        .expect("Duplicate session ID");

    actor
        .send(AddSession(2))
        .await
        .unwrap()
        .expect("Duplicate session ID");

    let count = actor.send(GetSessionCount).await.unwrap();
    assert!(count == 2, "2 sessions should have been added");

    let sessions: Vec<usize> = actor.send(GetConnectedSessions).await.unwrap();
    assert!(sessions.len() == 2, "2 sessions should have been added AND returned from the `GetConnectedSessions` message");

    let id = actor.send(GetSessionById(1)).await.unwrap();
    assert!(id.is_some(), "Session with id `1` should have been added");

    let invalid_id = actor.send(GetSessionById(50)).await.unwrap();
    assert!(
        invalid_id.is_none(),
        "No session with id `50` should be present"
    );

    let duplicate_session = actor.send(AddSession(1)).await.unwrap();

    assert!(
        duplicate_session.is_err(),
        "Session with id `1` should already have been inserted"
    );
}
