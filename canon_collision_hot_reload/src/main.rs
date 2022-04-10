use std::env;
use std::io::Read;
use std::io::Write;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::mpsc::Receiver;

use hotwatch::{Event, Hotwatch};

use canon_collision_lib::replays_files;

fn main() {
    let (tx, rx) = std::sync::mpsc::channel();

    // run once
    tx.send(Event::Write(PathBuf::new())).unwrap();

    // run on file change
    let mut hotwatch = Hotwatch::new().unwrap();
    let clone_tx = tx.clone();
    hotwatch
        .watch("../canon_collision", move |event| {
            clone_tx.send(event).unwrap();
        })
        .unwrap();
    hotwatch
        .watch("../canon_collision_lib", move |event| {
            tx.send(event).unwrap();
        })
        .unwrap();

    run(rx);
}

fn run(rx: Receiver<Event>) {
    let mut process: Option<Child> = None;
    let profile_arg = env!("PROFILE");

    for event in rx {
        if let Event::Write(_) | Event::Create(_) = event {
            let mut args = env::args();
            args.next();
            let args: Vec<String> = args.collect();
            let pass_through_args: Vec<&str> = args.iter().map(|x| x.as_ref()).collect();

            let build_status = if env!("PROFILE") == "release" {
                Command::new("cargo")
                    .current_dir("../canon_collision")
                    //.args(&["build", "-Z", "unstable-options", "--profile", &profile_arg]) // TODO: when --profile is stablized we can use that which is much nicer
                    .args(&["build", "--release"])
                    .status()
                    .unwrap()
            } else {
                Command::new("cargo")
                    .current_dir("../canon_collision")
                    .args(&["build"])
                    .status()
                    .unwrap()
            };

            // only try to launch if the build currently succeeds
            if build_status.success() {
                // if the process is running then hot reload it.
                // otherwise launch from scratch
                if is_process_running(&mut process) {
                    // This doesnt block because the replay save needs to be delayed until the Game::step where it has access to input data.
                    assert!(send_to_cc(":save_replay"));
                    // This blocks on the first command because we cant run another command until the next Game::step has occured.
                    assert!(send_to_cc(":help"));

                    let replays = replays_files::get_replay_names();
                    assert_ne!(replays.len(), 0, "replay was missing for some reason");
                    let latest_replay = &replays[0];
                    let latest_replay_filename = format!("{}.zip", latest_replay);

                    if let Some(mut x) = process.take() { x.kill().unwrap() }

                    // relaunch
                    process = launch(profile_arg, &["--replay", &latest_replay_filename]);

                    // busy loop until the replay is loaded or the process died, probably due to changes in the replay structure.
                    while !send_to_cc(":help") && is_process_running(&mut process) {}

                    // cleanup the replay
                    replays_files::delete_replay(latest_replay);
                } else {
                    process = launch(profile_arg, &pass_through_args);
                }
            }
        }
    }
}

fn is_process_running(process: &mut Option<Child>) -> bool {
    process
        .as_mut()
        .map(|x| x.try_wait().unwrap().is_none())
        .unwrap_or(false)
}

fn launch(profile_arg: &str, pass_through_args: &[&str]) -> Option<Child> {
    // TODO: use --profile arg instead of if when its stabilized
    if profile_arg == "release" {
        Command::new("cargo")
            .current_dir("../canon_collision")
            .args(&["run", "--release", "--", "--maxhistoryframes", "14400"])
            .args(pass_through_args)
            .spawn()
            .ok()
    } else {
        Command::new("cargo")
            .current_dir("../canon_collision")
            .args(&["run", "--", "--maxhistoryframes", "14400"])
            .args(pass_through_args)
            .spawn()
            .ok()
    }
}

/// returns true on success
fn send_to_cc(message: &str) -> bool {
    match TcpStream::connect("127.0.0.1:1613") {
        Ok(mut stream) => {
            stream
                .write_all(format!("C{}", message).as_bytes())
                .unwrap();

            // We need to receive to ensure we block, but we dont really care what the response is.
            let mut result = String::new();
            stream.read_to_string(&mut result).is_ok()
        }
        Err(_) => false,
    }
}
