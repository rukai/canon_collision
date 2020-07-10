use std::env;
use std::io::Read;
use std::io::Write;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Child};
use std::sync::mpsc::Receiver;

use hotwatch::{Hotwatch, Event};

use canon_collision_lib::replays_files;

fn main() {
    let (tx, rx) = std::sync::mpsc::channel();

    // run once
    tx.send(Event::Write(PathBuf::new())).unwrap();

    // run on file change
    let mut hotwatch = Hotwatch::new().unwrap();
    let clone_tx = tx.clone();
    hotwatch.watch("../canon_collision", move |event| {
        clone_tx.send(event).unwrap();
    }).unwrap();
    hotwatch.watch("../canon_collision_lib", move |event| {
        tx.send(event).unwrap();
    }).unwrap();

    run(rx);
}

fn run(rx: Receiver<Event>) {
    let mut process: Option<Child> = None;

    for event in rx {
        if let Event::Write (_) | Event::Create (_) = event {
            let mut args = env::args();
            args.next();
            let args: Vec<String> = args.collect();
            let pass_through_args: Vec<&str> = args.iter().map(|x| x.as_ref()).collect();

            let build_status = Command::new("cargo")
                .current_dir("../canon_collision")
                .args(&["build", "--release"])
                .status()
                .unwrap();

            // only try to launch if the build currently succeeds
            if build_status.success() {
                // if the process is running then hot reload it.
                // otherwise launch from scratch
                if process.as_mut().map(|x| x.try_wait().unwrap().is_none()).unwrap_or(false) {
                    // This doesnt block because the replay save needs to be delayed until the Game::step where it has access to input data.
                    assert!(send_to_cc(":save_replay"));
                    // This blocks on the first command because we cant run another command until the next Game::step has occured.
                    assert!(send_to_cc(":help"));

                    let replays = replays_files::get_replay_names();
                    assert_ne!(replays.len(), 0, "replay was missing for some reason");
                    let latest_replay = &replays[0];
                    let latest_replay_filename = format!("{}.zip", latest_replay);

                    process.take().map(|mut x| x.kill().unwrap());

                    // relaunch
                    process = launch(&["--replay", &latest_replay_filename]);

                    // busy loop until the replay is loaded
                    while !send_to_cc(":help") { }

                    // cleanup the replay
                    replays_files::delete_replay(latest_replay);
                }
                else {
                    process = launch(&pass_through_args);
                }
            }
        }
    }
}

fn launch(pass_through_args: &[&str]) -> Option<Child> {
    Command::new("cargo")
        .current_dir("../canon_collision")
        .args(&["run", "--release", "--"])
        .args(pass_through_args)
        .spawn()
        .ok()
}

/// returns true on success
fn send_to_cc(message: &str) -> bool {
    match TcpStream::connect("127.0.0.1:1613") {
        Ok(mut stream) => {
            stream.write(format!("C{}", message).as_bytes()).unwrap();

            // We need to receive to ensure we block, but we dont really care what the response is.
            let mut result = String::new();
            stream.read_to_string(&mut result).is_ok()
        }
        Err(_) => false,
    }
}
