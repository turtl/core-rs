//! The Event module defines an Emitter struct/implementation that builds on the
//! json `Value` to provide arbitrary arguments to bound functions.
//!
//! We also define an event struct for triggering events.
//!
//! Note that our bindings are wrapped in an RwLock, so an evented object can be
//! bound/triggered from any thread.

use ::std::sync::RwLock;
use ::std::collections::HashMap;

use ::jedi::Value;

/// Defines what type of binding we have
enum BindType {
    Every,
    Once,
}

/// Define a trait for our event callbacks.
pub trait EventThunk: Send + Sync + 'static {
    fn call_box(&self, &Value);
}
impl<F: Fn(&Value) + Send + Sync + 'static> EventThunk for F {
    fn call_box(&self, val: &Value) {
        (*self)(val);
    }
}

/// Holds information about a callback.
pub struct Callback {
    cb: Box<EventThunk>,
    binding: BindType,
    name: String,
}

/// An alias to make returning the bindings object easier
pub type Bindings = RwLock<HashMap<String, Vec<Callback>>>;

/// The Emitter class holds a set of event bindings. It implements the `Emitter`
/// trait and can be used as a standalone event emitter object.
pub struct EventEmitter {
    _bindings: Bindings,
}

/// Defines an interface for an event emitter, including binding/triggering
/// events. The only non-provided method is the `bindings` function, which has
/// to return a mutable reference to a HashMap of bindings.
pub trait Emitter {
    /// Grab a mutable ref to this emitter's bindings
    fn bindings(&self) -> &Bindings;

    /// Binds a callback to an event name.
    fn do_bind(&self, event_name: &str, cb: Callback) {
        // make sure we unbind ANY callbacks with the same event name/ref name
        // as this one, effectively making it so the same name/name pair will
        // *replace* existing bindings.
        self.unbind(event_name, cb.name.as_str());
        let bindings = self.bindings();
        let mut guard = bindings.write().unwrap();
        let events = guard.entry(String::from(event_name)).or_insert(Vec::with_capacity(3));
        events.push(cb);
    }

    /// Bind a callback to an event name. The binding takes a name, which makes
    /// it easy to unbind later (by name).
    fn bind<F>(&self, event_name: &str, cb: F, bind_name: &str)
        where F: Fn(&Value) + Send + Sync + 'static
    {
        self.do_bind(event_name, Callback {
            cb: Box::new(cb),
            binding: BindType::Every,
            name: String::from(bind_name),
        });
    }

    /// Bind a ont-time callback to an event name. The binding takes a name,
    /// which makes it easy to unbind later (by name).
    fn bind_once<F>(&self, event_name: &str, cb: F, bind_name: &str)
        where F: Fn(&Value) + Send + Sync + 'static
    {
        self.do_bind(event_name, Callback {
            cb: Box::new(cb),
            binding: BindType::Once,
            name: String::from(bind_name),
        });
    }

    /// Unbind an event/listener from thie emitter.
    fn unbind(&self, event_name: &str, bind_name: &str) -> bool {
        let bindings = self.bindings();
        let mut guard = bindings.write().unwrap();
        match guard.get_mut(event_name) {
            Some(x) => {
                let mut removed = false;
                for idx in (0..(x.len())).rev() {
                    if &x[idx].name == bind_name {
                        x.remove(idx);
                        removed = true;
                    }
                }
                removed
            }
            None => false
        }
    }

    /// Trigger an event. Any function bound to the event name gets fired, with
    /// `data` passed as the only argument.
    fn trigger(&self, event_name: &str, data: &Value) -> () {
        let bindings = self.bindings();
        let mut guard = bindings.write().unwrap();
        match guard.get_mut(event_name) {
            Some(x) => {
                let mut removes: Vec<usize> = Vec::new();
                for idx in 0..(x.len()) {
                    let callback = &x[idx];
                    let cb = &callback.cb;
                    let binding = &callback.binding;
                    cb.call_box(&data);
                    match *binding {
                        BindType::Once => {
                            removes.push(idx);
                        }
                        _ => (),
                    }
                }
                // we want 3,2,1 instead of 1,2,3 so our indexing is preserved
                // as we iterate over elements
                removes.reverse();
                for idx in removes {
                    x.remove(idx);
                }
            }
            None => (),
        }
    }
}

impl EventEmitter {
    /// Make a new Emitter.
    pub fn new() -> EventEmitter {
        EventEmitter { _bindings: RwLock::new(HashMap::new()) }
    }
}

impl Emitter for EventEmitter {
    fn bindings(&self) -> &Bindings {
        &self._bindings
    }
}

impl Default for EventEmitter {
    fn default() -> EventEmitter {
        EventEmitter::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::jedi::{self, Value};
    use std::sync::{Arc, RwLock};

    #[test]
    fn bind_emit() {
        let data = Arc::new(RwLock::new(vec![0]));
        let jval = jedi::parse(&String::from(r#"{"name":"larry"}"#)).unwrap();
        let rdata = data.clone();
        {
            let data = data.clone();
            let cb = move |x: &Value| {
                assert_eq!(jedi::stringify(x).unwrap(), r#"{"name":"larry"}"#);
                data.write().unwrap()[0] += 1;
            };
            let mut emitter = EventEmitter::new();
            //let data = data.clone();
            emitter.bind("fire", cb, "test:fire");
            //emitter.bind("omg", cb, "test:test");

            assert_eq!(rdata.read().unwrap()[0], 0);
            emitter.trigger("hellp", &jval);
            assert_eq!(rdata.read().unwrap()[0], 0);
            emitter.trigger("fire", &jval);
            assert_eq!(rdata.read().unwrap()[0], 1);
            emitter.trigger("fire", &jval);
            assert_eq!(rdata.read().unwrap()[0], 2);
        }
    }

    #[test]
    fn bind_once_emit() {
        let data = Arc::new(RwLock::new(vec![0]));
        let jval = jedi::parse(&String::from(r#"{"name":"larry"}"#)).unwrap();
        let rdata = data.clone();
        {
            let data = data.clone();
            let cb = move |x: &Value| {
                assert_eq!(jedi::stringify(x).unwrap(), r#"{"name":"larry"}"#);
                data.write().unwrap()[0] += 1;
            };
            let emitter = EventEmitter::new();
            //let data = data.clone();
            emitter.bind_once("fire", cb, "test:fire");

            assert_eq!(rdata.read().unwrap()[0], 0);
            emitter.trigger("hellp", &jval);
            assert_eq!(rdata.read().unwrap()[0], 0);
            emitter.trigger("fire", &jval);
            assert_eq!(rdata.read().unwrap()[0], 1);
            emitter.trigger("fire", &jval);
            assert_eq!(rdata.read().unwrap()[0], 1);
        }
    }

    #[test]
    fn replace() {
        let data = Arc::new(RwLock::new(vec![0]));
        let jval = jedi::obj();
        let rdata = data.clone();
        {
            let data1 = data.clone();
            let emitter = EventEmitter::new();
            emitter.bind("fire", move |_| {
                data1.write().unwrap()[0] += 1;
            }, "omglolwtf");
            assert_eq!(rdata.read().unwrap()[0], 0);
            emitter.trigger("fire", &jval);
            assert_eq!(rdata.read().unwrap()[0], 1);
            // replace with cb that does nothing. NO-THING.
            emitter.bind("fire", move |_| { }, "omglolwtf");
            emitter.trigger("fire", &jval);
            // should still be 1
            assert_eq!(rdata.read().unwrap()[0], 1);
        }
    }

    #[test]
    fn unbind() {
        let data = Arc::new(RwLock::new(vec![0]));
        let jval = jedi::parse(&String::from(r#"{"name":"larry"}"#)).unwrap();
        let rdata = data.clone();
        {
            let data = data.clone();
            let cb = move |x: &Value| {
                assert_eq!(jedi::stringify(x).unwrap(), r#"{"name":"larry"}"#);
                data.write().unwrap()[0] += 1;
            };
            let emitter = EventEmitter::new();
            //let data = data.clone();
            emitter.bind("fire", cb, "test:fire");

            assert_eq!(rdata.read().unwrap()[0], 0);
            emitter.trigger("fire", &jval);
            assert_eq!(rdata.read().unwrap()[0], 1);
            emitter.trigger("fire", &jval);
            assert_eq!(rdata.read().unwrap()[0], 2);

            emitter.unbind("fire", "test:fire");

            emitter.trigger("fire", &jval);
            assert_eq!(rdata.read().unwrap()[0], 2);
        }
    }
}

