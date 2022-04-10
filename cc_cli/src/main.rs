use std::env;
use std::io::Read;
use std::io::Write;
use std::net::TcpStream;

fn main() {
    std::process::exit(main_main());
}

fn main_main() -> i32 {
    let mut args = env::args();
    args.next();
    let out_vec: Vec<String> = args.collect();
    let out: String = format!("C{}", out_vec.join(" "));

    match TcpStream::connect("127.0.0.1:1613") {
        Ok(mut stream) => {
            stream.write_all(out.as_bytes()).unwrap();

            let mut result = String::new();
            if stream.read_to_string(&mut result).is_ok() {
                println!("{}", result);
            }
            0
        }
        Err(e) => {
            println!("Could not connect to Canon Collision host: {}", e);
            1
        }
    }
}
