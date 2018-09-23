extern crate jedi;
extern crate regex;
extern crate rustyline;
extern crate turtl_core;

use ::std::env;
use ::std::thread;
use ::std::time::Duration;
use jedi::Value;
use regex::Regex;
use rustyline::Editor;
use rustyline::error::ReadlineError;
use turtl_core::error::TResult;

pub fn sleep(millis: u64) {
    thread::sleep(Duration::from_millis(millis));
}

fn exit() {
    turtl_core::send(String::from(r#"["0","sync:shutdown",false]"#)).expect("client::exit() -- failed to send shutdown command");
    turtl_core::send(String::from(r#"["0","user:logout",false]"#)).expect("client::exit() -- failed to send logout command");
}

fn repl() -> TResult<()> {
    let re = Regex::new(r#"'.+?'|".+?"|[^ ]+"#).expect("client::repl() -- failed to create regex");
    let mut req_id = 1;
    let mut rl = Editor::<()>::new();
    // TODO: Find a good path for history
    //rl.load_history("history.txt")?;

    loop {
        let req_str = format!("{}", req_id);
        let readline = rl.readline(">> ");

        match readline {
            Ok(line) => {
                if line.is_empty() {
                    continue;
                }

                rl.add_history_entry(&line);

                let mut parts: Vec<String> = re.find_iter(&line)
                    .map(|x| String::from(x.as_str().trim()
                        .trim_matches('"').trim_matches('\'')))
                    .collect::<Vec<_>>();

                if parts.len() == 0 {
                    continue;
                }

                let cmd = parts.remove(0);

                // I GUESS I'll let you exit
                if cmd == "quit" || cmd == "q" {
                    exit();
                    break;
                }

                let mut msg_parts: Vec<Value> = vec![Value::String(req_str.clone()), Value::String(cmd)];
                let mut args: Vec<Value> = parts.into_iter()
                    .map(|x| {
                        match jedi::parse::<Value>(&x) {
                            Ok(val) => val,
                            Err(_) => Value::String(format!("{}", x)),
                        }
                    })
                .collect::<Vec<_>>();
                msg_parts.append(&mut args);

                let msg = jedi::stringify(&msg_parts)?;
                turtl_core::send(msg).expect("client::repl() -- failed to send core message");
                let response = turtl_core::recv(None).expect("client::repl() -- failed to recv core message");
                println!("response: {}", response);
                req_id += 1;
            },
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                exit();
                break;
            },
            Err(err) => {
                eprintln!("Error: {:?}", err);
                exit();
                break;
            }
        }
    }
    Ok(())
}

fn main() {
    if env::var("TURTL_CONFIG_FILE").is_err() {
        env::set_var("TURTL_CONFIG_FILE", "../config.yaml");
    }
    let handle = turtl_core::start(String::from(r#"{"messaging":{"reqres_append_mid":false}}"#));

    sleep(1000);
    println!("");
    println!("");
    println!("Welcome to the Turtl Client.");
    println!("");
    match repl() {
        Ok(_) => {},
        Err(err) => println!("turtl-client::repl() -- {}", err),
    }

    handle.join().expect("client::main() -- failed to join thread handle");
}

