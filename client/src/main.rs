extern crate jedi;
extern crate rustyline;
extern crate turtl_core;

use ::std::env;
use ::std::thread;
use ::std::time::Duration;
use jedi::Value;
use rustyline::Editor;
use rustyline::error::ReadlineError;
use turtl_core::error::TResult;

pub fn sleep(millis: u64) {
    thread::sleep(Duration::from_millis(millis));
}

fn exit() {
    turtl_core::send(String::from(r#"["0","sync:shutdown",false]"#)).unwrap();
    turtl_core::send(String::from(r#"["0","user:logout",false]"#)).unwrap();
}

fn repl() -> TResult<()> {
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

                let mut parts: Vec<String> = line.split(" ")
                    .filter(|x| x != &"")
                    .map(|x| String::from(x.trim()))
                    .collect::<Vec<_>>();

                if parts.len() == 0 {
                    continue;
                }

                let cmd = parts.remove(0);

                // i GUESS i'll let you exit
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
                turtl_core::send(msg).unwrap();
                let response = turtl_core::recv(None).unwrap();
                println!("response: {}", response);
                req_id += 1;
            },
            Err(ReadlineError::Interrupted) => {
                exit();
                break;
            },
            Err(ReadlineError::Eof) => {
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

    handle.join().unwrap();
}

