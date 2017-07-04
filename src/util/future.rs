//! This zany module is responsible for running and polling futures, much like
//! an event loop would. Note that this is all required (or so I believe)
//! becuase Future.forget() was removed from futures-es. So what this does is
//! essentially adds futures to a list to poll at a later time, as well as
//! signals the main thread to poll the futures on a somewhat-intelligent
//! timeline.

use ::std::sync::{Arc, RwLock};
use ::std::thread;

use ::futures::{Future, BoxFuture, Async};
use ::futures::executor::{self, Spawn, Notify};
use ::util;
use ::util::thredder::Pipeline;

lazy_static! {
    /// Holds a list of callbacks that each poll a future and return it if it
    /// needs to be polled again or None if the future has completed
    static ref FUTURES: RwLock<Vec<FutureCb>> = RwLock::new(Vec::new());
}

/// An easy type alias for type the future callbacks return
type CbReturn = Option<FutureTask>;

/// A one-time function (essentially defines our future callbacks)
trait Thunk: Send + 'static {
    fn call_box(self: Box<Self>) -> CbReturn;
}
impl<F: FnOnce() -> CbReturn + Send + 'static> Thunk for F {
    fn call_box(self: Box<Self>) -> CbReturn {
        (*self)()
    }
}

/// Holds a future callback
struct FutureCb {
    cb: Box<Thunk>,
}
impl FutureCb {
    /// Create a new future callback, given a function that returns `CbReturn`
    fn new<F>(cb: F) -> FutureCb
        where F: FnOnce() -> CbReturn + Send + 'static
    {
        FutureCb {
            cb: Box::new(cb),
        }
    }

    /// Call the function this callback holds. This consumes the FutureCb.
    fn call(self) -> CbReturn {
        self.cb.call_box()
    }
}

/// Needed so the compiler stops bitching that my Thunk type can't be shared
/// across threads. It very well might be true that it CANNOT be shared across
/// threads, but because this module is only ever called into from the main
/// thread, I'm not concerned with what the compiler thinks. Here is the
/// override telling the compiler as much.
unsafe impl Sync for FutureCb {}

/// A crude implementation of Notify so we can drive our futures forward.
struct ThreadNotify { }
impl ThreadNotify {
    fn new() -> ThreadNotify { ThreadNotify {} }
}
impl Notify for ThreadNotify {
    fn notify(&self, _id: usize) { }
}

/// Holds a future+task combo, using the Spawn/ThreadNotify types.
struct FutureTask {
    id: usize,
    spawn: Spawn<BoxFuture<(), ()>>,
    notify: Arc<ThreadNotify>,
}
impl FutureTask {
    /// Create a new FutureTask, given a future.
    fn new<T>(future: T) -> FutureTask
        where T: Future + Send + 'static
    {
        // cast the future to BoxFuture<(), ()>
        let future = future.map(|_| ()).map_err(|_| ()).boxed();
        let notify = Arc::new(ThreadNotify::new());
        // this creates our task/future combo
        let spawn = executor::spawn(future);
        FutureTask {
            id: 0,
            spawn: spawn,
            notify: notify,
        }
    }

    /// Poll the Future/Task combo to drive it forward
    fn poll(&mut self) -> Result<Async<()>, ()> {
        self.spawn.poll_future_notify(&self.notify.clone(), self.id)
    }
}

/// Creates a callback that polls a FutureTask and returns the FutureTask if
/// the future needs to be polled again. This callback is added to the `pollers`
/// list.
///
/// This is what turns a FutureTask into a future callback.
fn poll_on_next(mut futuretask: FutureTask, pollers: &mut Vec<FutureCb>) {
    let cb = move || {
        match futuretask.poll() {
            Ok(x) => match x {
                Async::NotReady => {
                    Some(futuretask)
                },
                _ => None,
            },
            Err(_) => None,
        }
    };
    pollers.push(FutureCb::new(cb));
}

/// Poll all pending futures.
///
/// First we copy the future callbacks off our global FUTURES list, empty the
/// global list, and release our lock.
///
/// Then we loop over each (copied) callback, call it, and if it returns a
/// future, add that future to a new vector. Note that if a future is returned
/// from a callback, it means that it should be polled again (otherwise, if None
/// is returned, it means the future is finished and we can be done with it);
///
/// Once the futures are done being polled, we append what's in the global list
/// with our vector of returned futures and set the resulting list BACK into the
/// global list.
fn poll() {
    // copy the future callbacks off of our global list and into a local var
    // so we can get the data out without holding the lock open. note that we
    // normally would have to worry about other threads modifying the FUTURES
    // list while this function is running, but since only the main thread calls
    // it (EVER), we have nothing to really worry about.
    // NOTE: it's necessary to not hold open the lock because it's possible to
    // call run() while poll() is running (if running a future creates another
    // future (common)) which deadlocks.
    let list_copy = {
        let mut list = (*FUTURES).write().unwrap();
        // don't do anything if empty
        if list.len() == 0 { return }

        let mut tmp: Vec<FutureCb> = Vec::with_capacity(list.len());
        tmp.append(&mut list);
        // empty out list. we will want to append it back onto `next` after our
        // run so we catch any futures that were added while we poll
        (*list) = Vec::new();
        tmp
    };
    trace!("future::poll() -- polling {} futures", list_copy.len());
    // holds a list of futures that, after running, still need to be polled.
    // this will replace our global list once all futures have been polled.
    let mut next: Vec<FutureCb> = Vec::new();
    for cb in list_copy {
        let future = cb.call();
        if future.is_some() {
            poll_on_next(future.unwrap(), &mut next);
        }
    }

    // reset our list
    let mut list = (*FUTURES).write().unwrap();
    // append any futures that were added during polling
    next.append(&mut list);
    (*list) = next;
}

/// Schedule a future to be run.
pub fn run<T>(future: T)
    where T: Future + Send + 'static
{
    let mut list = (*FUTURES).write().unwrap();
    poll_on_next(FutureTask::new(future), &mut list);
}

/// Called ONCE before entering any main loop. This sets up polling for our
/// queued futures in the global FUTURES list. It tries to be smart about the
/// delays used when polling (1s if no futures are currently being polled, 10ms
/// if there are futures pending).
pub fn start_poll(tx: Pipeline) {
    let future_count = {
        let list = (*FUTURES).read().unwrap();
        list.len()
    };
    thread::spawn(move || {
        let ms = if future_count > 0 { 10 } else { 1000 };
        util::sleep(ms);
        let tx2 = tx.clone();
        tx.next(|_| {
            poll();
            start_poll(tx2);
        });
    });
}

