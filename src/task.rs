use futures::unsync::oneshot;


pub struct Task<T> where T: Clone {
    waiters: Vec<oneshot::Sender<T>>,
}

impl<T> Task<T> where T: Clone {

    pub fn new() -> Task<T> {
        Task { waiters: Vec::new() }
    }

    pub fn wait(&mut self) -> oneshot::Receiver<T> {
        let (tx, rx) = oneshot::channel();
        self.waiters.push(tx);
        rx
    }

    pub fn set_result(self, result: T) {
        for waiter in self.waiters {
            let _ = waiter.send(result.clone());
        }
    }
}
