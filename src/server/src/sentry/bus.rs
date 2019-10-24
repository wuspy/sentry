use futures::sync::mpsc::{unbounded, SendError, UnboundedReceiver, UnboundedSender};
use futures::{Poll, Sink, StartSend, Stream};
use std::sync::Mutex;
use tokio::prelude::Async;

pub struct BusSender<T: Clone> {
    sender: UnboundedSender<T>,
}

impl<T: Clone> BusSender<T> {
    fn new(sender: UnboundedSender<T>) -> Self {
        BusSender { sender }
    }

    pub fn unbounded_send(&self, msg: T) -> Result<(), SendError<T>> {
        self.sender.unbounded_send(msg)
    }
}

impl<T: Clone> Sink for BusSender<T> {
    type SinkItem = T;
    type SinkError = SendError<T>;

    fn start_send(&mut self, msg: T) -> StartSend<T, SendError<T>> {
        self.sender.start_send(msg)
    }

    fn poll_complete(&mut self) -> Poll<(), SendError<T>> {
        self.sender.poll_complete()
    }

    fn close(&mut self) -> Poll<(), SendError<T>> {
        self.sender.close()
    }
}

impl<T: Clone> Clone for BusSender<T> {
    fn clone(&self) -> Self {
        BusSender {
            sender: self.sender.clone(),
        }
    }
}

pub struct BusReceiver<T: Clone> {
    receiver: UnboundedReceiver<T>,
    clones: Mutex<Vec<UnboundedSender<T>>>,
}

impl<T: Clone> BusReceiver<T> {
    fn new(receiver: UnboundedReceiver<T>) -> Self {
        BusReceiver {
            receiver,
            clones: Mutex::new(Vec::new()),
        }
    }
}

impl<T: Clone> Clone for BusReceiver<T> {
    fn clone(&self) -> Self {
        let (sender, receiver) = unbounded::<T>();
        self.clones.lock().unwrap().push(sender);
        BusReceiver::new(receiver)
    }
}

impl<T: Clone> Stream for BusReceiver<T> {
    type Item = T;
    type Error = ();

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        let result = self.receiver.poll();
        if let Ok(Async::Ready(Some(msg))) = &result {
            for sender in self.clones.lock().unwrap().iter() {
                // TODO handle this
                sender.unbounded_send(msg.clone());
            }
        }
        result
    }
}

pub type Bus<T> = (BusSender<T>, BusReceiver<T>);

pub fn new<T: Clone>() -> Bus<T> {
    let (sink, stream) = unbounded::<T>();
    (BusSender::new(sink), BusReceiver::new(stream))
}
