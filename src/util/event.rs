//! The Event module defines an Emitter struct/implementation that builds on the
//! json `Value` to provide arbitrary arguments to bound functions.
//!
//! We also define an event struct for triggering events.

use ::std::collections::HashMap;
use ::util::json::Value;

/// Define an easy Callback type for us
pub type CallbackType = Fn(&Value) + 'static;

/// Defines what type of binding we have
enum BindType {
    Every,
    Once,
}

/// Holds information about a callback.
pub struct Callback {
    cb: Box<CallbackType>,
    binding: BindType,
    name: String,
}

/// The Emitter class holds a set of event bindings. It implements the `Emitter`
/// trait and can be used as a standalone event emitter object.
pub struct EventEmitter {
    _bindings: HashMap<String, Vec<Callback>>,
}

/// Defines an interface for an event emitter, including binding/triggering
/// events. The only non-provided method is the `bindings` function, which has
/// to return a mutable reference to a HashMap of bindings.
pub trait Emitter {
    /// Grab a mutable ref to this emitter's bindings
    fn bindings(&mut self) -> &mut HashMap<String, Vec<Callback>>;

    /// Binds a callback to an event name.
    fn do_bind(&mut self, name: &str, cb: Callback) {
        let mut bindings = self.bindings();
        if bindings.contains_key(name) {
            match bindings.get_mut(name) {
                Some(x) => x.push(cb),
                None => (),
            }
        } else {
            let events = vec![cb];
            bindings.insert(String::from(name), events);
        }
    }

    /// Bind a callback to an event name. The binding takes a name, which makes
    /// it easy to unbind later (by name).
    fn bind<F>(&mut self, event_name: &str, cb: F, bind_name: &str)
        where F: Fn(&Value) + 'static
    {
        self.do_bind(event_name, Callback {
            cb: Box::new(cb),
            binding: BindType::Every,
            name: String::from(bind_name),
        });
    }

    /// Bind a ont-time callback to an event name. The binding takes a name,
    /// which makes it easy to unbind later (by name).
    fn bind_once<F>(&mut self, event_name: &str, cb: F, bind_name: &str)
        where F: Fn(&Value) + 'static
    {
        self.do_bind(event_name, Callback {
            cb: Box::new(cb),
            binding: BindType::Once,
            name: String::from(bind_name),
        });
    }

    /// Unbind an event/listener from thie emitter.
    fn unbind(&mut self, event_name: &str, bind_name: &str) -> bool {
        let mut bindings = self.bindings();
        match bindings.get_mut(event_name) {
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
    fn trigger(&mut self, event_name: &str, data: &Value) -> () {
        let mut bindings = self.bindings();
        match bindings.get_mut(event_name) {
            Some(x) => {
                let mut removes: Vec<usize> = Vec::new();
                for idx in 0..(x.len()) {
                    let callback = &x[idx];
                    let cb = &callback.cb;
                    let binding = &callback.binding;
                    cb(&data);
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
        EventEmitter { _bindings: HashMap::new() }
    }
}

impl Emitter for EventEmitter {
    fn bindings(&mut self) -> &mut HashMap<String, Vec<Callback>> {
        &mut self._bindings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::util::json::{self, Value};
    use std::sync::{Arc, RwLock};

    #[test]
    fn bind_emit() {
        let data = Arc::new(RwLock::new(vec![0]));
        let jval = json::parse(&String::from(r#"{"name":"larry"}"#)).unwrap();
        let rdata = data.clone();
        {
            let data = data.clone();
            let cb = move |x: &Value| {
                assert_eq!(json::stringify(x).unwrap(), r#"{"name":"larry"}"#);
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
        let jval = json::parse(&String::from(r#"{"name":"larry"}"#)).unwrap();
        let rdata = data.clone();
        {
            let data = data.clone();
            let cb = move |x: &Value| {
                assert_eq!(json::stringify(x).unwrap(), r#"{"name":"larry"}"#);
                data.write().unwrap()[0] += 1;
            };
            let mut emitter = EventEmitter::new();
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
    fn unbind() {
        let data = Arc::new(RwLock::new(vec![0]));
        let jval = json::parse(&String::from(r#"{"name":"larry"}"#)).unwrap();
        let rdata = data.clone();
        {
            let data = data.clone();
            let cb = move |x: &Value| {
                assert_eq!(json::stringify(x).unwrap(), r#"{"name":"larry"}"#);
                data.write().unwrap()[0] += 1;
            };
            let mut emitter = EventEmitter::new();
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

