//! The Event module defines an Emitter struct/implementation that builds on the
//! json `Value` to provide arbitrary arguments to bound functions.
//!
//! We also define an event struct for triggering events.

use ::std::collections::HashMap;
use ::util::json::Value;

/// Define an easy Callback type for us
pub type CallbackType<'a> = Fn(&Value) -> () + 'a;

enum BindType {
    Every,
    Once,
}

struct Callback<'a> {
    cb: &'a CallbackType<'a>,
    binding: BindType,
    name: &'a str,
}

pub struct Emitter<'a> {
    bindings: HashMap<&'a str, Vec<Callback<'a>>>,
}

impl<'a> Emitter<'a> {
    pub fn new() -> Emitter<'a> {
        Emitter { bindings: HashMap::new() }
    }

    fn do_bind(&mut self, name: &'a str, cb: Callback<'a>) -> () {
        if self.bindings.contains_key(name) {
            match self.bindings.get_mut(name) {
                Some(x) => x.push(cb),
                None => (),
            }
        } else {
            let events = vec![cb];
            self.bindings.insert(name, events);
        }
    }

    pub fn bind(&mut self, event_name: &'a str, cb: &'a CallbackType, bind_name: &'a str) -> () {
        self.do_bind(event_name, Callback {
            cb: cb,
            binding: BindType::Every,
            name: bind_name,
        });
    }

    pub fn bind_once(&mut self, event_name: &'a str, cb: &'a CallbackType, bind_name: &'a str) -> () {
        self.do_bind(event_name, Callback {
            cb: cb,
            binding: BindType::Once,
            name: bind_name,
        });
    }

    pub fn unbind(&mut self, event_name: &str, bind_name: &str) -> bool {
        match self.bindings.get_mut(event_name) {
            Some(x) => {
                let mut removed = false;
                for idx in (0..(x.len())).rev() {
                    let callback_name = x[idx].name;
                    if callback_name == bind_name {
                        x.remove(idx);
                        removed = true;
                    }
                }
                removed
            }
            None => false
        }
    }

    pub fn trigger(&mut self, event_name: &str, data: &Value) -> () {
        match self.bindings.get_mut(event_name) {
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
            let mut emitter = Emitter::new();
            //let data = data.clone();
            emitter.bind("fire", &cb, "test:fire");

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
            let mut emitter = Emitter::new();
            //let data = data.clone();
            emitter.bind_once("fire", &cb, "test:fire");

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
            let mut emitter = Emitter::new();
            //let data = data.clone();
            emitter.bind("fire", &cb, "test:fire");

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

