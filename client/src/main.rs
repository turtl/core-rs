include!("../../tests/lib/util.rs");
use ::std::io::{self, Write};

fn repl() -> TResult<()> {
    let mut req_id = 1;
    loop {
        let req_str = format!("{}", req_id);
        io::stdout().write(&String::from("> ").as_bytes())?;
        io::stdout().flush()?;
        let mut cmd = String::new();
        io::stdin().read_line(&mut cmd)?;

        let mut parts: Vec<String> = cmd.as_str().split(" ")
            .filter(|x| x != &"")
            .map(|x| String::from(x.trim()))
            .collect::<Vec<_>>();
        if parts.len() == 0 { continue; }

        let cmd = parts.remove(0);

        // i GUESS i'll let you exit
        if cmd == "quit" || cmd == "q" {
            send(format!("[\"{}\",\"app:shutdown\"]", req_id).as_str());
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
        send_msg(&msg.as_str())?;
        let response = recv_msg(req_str.as_str())?;
        println!("response: {}", response);
        req_id += 1;
    }
    Ok(())
}

fn main() {
    if env::var("TURTL_CONFIG_FILE").is_err() {
        env::set_var("TURTL_CONFIG_FILE", "../config.yaml");
    }
    let handle = init();

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

