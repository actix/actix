use futures::unsync::oneshot;


pub struct Condition<T> where T: Clone {
    waiters: Vec<oneshot::Sender<T>>,
}

impl<T> Condition<T> where T: Clone {

    pub fn new() -> Condition<T> {
        Condition { waiters: Vec::new() }
    }

    pub fn wait(&mut self) -> oneshot::Receiver<T> {
        let (tx, rx) = oneshot::channel();
        self.waiters.push(tx);
        rx
    }

    pub fn set(self, result: T) {
        for waiter in self.waiters {
            let _ = waiter.send(result.clone());
        }
    }
}
