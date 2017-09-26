use futures::unsync::oneshot;

pub struct Condition<T> where T: Clone {
    waiters: Vec<oneshot::Sender<T>>,
}

impl<T> Condition<T> where T: Clone {

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

impl<T> Default for Condition<T> where T: Clone {
    fn default() -> Self {
        Condition { waiters: Vec::new() }
    }
}
