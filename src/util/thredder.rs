//! Thredder is a thread tracking system that not only creates threads for
//! specific purposes, but sets up communication channels between the threads
//! and tracks the state of them.

use ::std::thread::{self, JoinHandle};
use ::std::sync::mpsc::{self, Sender, Receiver, RecvError};
use ::std::error::Error;

use ::error::{TResult, TError};
use ::util::reqres::{ReqRes, Request, Response};

/// Stores state information for a thread we've spawned
pub struct Thredder {
    /// Our Thredder's name
    pub name: String,
    /// Allows sending messages to our thread
    pub tx: Sender<Request>,
    /// The thread handle
    pub handle: JoinHandle<()>,
}

impl Thredder {
    /// Create a new Thredder
    fn new(tx: Sender<Request>, handle: JoinHandle<()>, name: String) -> Thredder {
        Thredder {
            tx: tx,
            handle: handle,
            name: name,
        }
    }

    /// Send a request out to this thread, calling `cb` when we get a response
    fn request<F>(&mut self, reqres: &mut ReqRes, request: String, cb: F)
        where F: Fn(Vec<u8>) + 'static
    {
        let req = reqres.request(request, cb);
        self.tx.send(req);
    }
}

fn send_error(name: &String, tx: Sender<Response>, error: TError) {
    error!("thread: {}: recv error: {}", name, error);
    //tx.send();
}

/// Spawn a thread
pub fn spawn<F>(name: &str, tx_main: Sender<Response>, dispatch: F) -> Thredder
    where F: Fn(Sender<Response>, Request) -> TResult<()> + ::std::marker::Send + 'static
{
    let (tx, rx) = mpsc::channel();
    let name = String::from(name);
    let handle = {
        let tx = ();
        let name = String::from(&name[..]);
        thread::spawn(move || {
            let mut poll = true;
            while poll {
                let recv: TResult<Request> = rx.recv().map_err(|e| toterr!(e));
                match recv.and_then(|req| {
                    match req.request.as_ref() {
                        "quit" => Err(TError::Shutdown),
                        _ => dispatch(tx_main.clone(), req),
                    }
                }) {
                    Ok(x) => (),
                    Err(e) => match e {
                        TError::Shutdown => { poll = false },
                        _ => send_error(&name, tx_main.clone(), e),
                    }
                }
            }
        })
    };
    Thredder::new(tx, handle, name)
}

