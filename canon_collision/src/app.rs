use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};

use crate::ai;
use crate::audio::Audio;
use crate::camera::Camera;
use crate::cli::{CLIResults, ContinueFrom};
use crate::game::{Edit, Game, GameSetup, GameState, PlayerSetup};
use crate::graphics::GraphicsMessage;
use crate::menu::{Menu, MenuState, ResumeMenu};
use crate::replays;
use crate::rules::Rules;
use canon_collision_lib::assets::Assets;
use canon_collision_lib::command_line::CommandLine;
use canon_collision_lib::config::Config;
use canon_collision_lib::input::Input;
use canon_collision_lib::network::{NetCommandLine, Netplay, NetplayState};
use canon_collision_lib::package::Package;

use std::sync::mpsc::channel;
use std::thread;
use std::time::{Duration, Instant};
use winit::event::WindowEvent;
use winit_input_helper::WinitInputHelper;

pub fn run_in_thread(
    cli_results: CLIResults,
) -> (Sender<WindowEvent<'static>>, Receiver<GraphicsMessage>) {
    let (render_tx, render_rx) = channel();
    let (event_tx, event_rx) = mpsc::channel();
    thread::spawn(move || {
        run(cli_results, event_rx, render_tx);
    });
    (event_tx, render_rx)
}

fn run(
    mut cli_results: CLIResults,
    event_rx: Receiver<WindowEvent<'static>>,
    render_tx: Sender<GraphicsMessage>,
) {
    let mut config = Config::load();
    if let ContinueFrom::Close = cli_results.continue_from {
        return;
    }

    let mut input = Input::new();
    let mut net_command_line = NetCommandLine::new();
    let mut netplay = Netplay::new();

    let mut package = if let Some(path) = Package::find_package_in_parent_dirs() {
        if let Some(package) = Package::open(path) {
            Some(package)
        } else {
            println!("Could not load package");
            return;
        }
    } else {
        println!("Could not find package/ in current directory or any of its parent directories.");
        return;
    };

    // package has better file missing error handling so load assets after package
    let assets = if let Some(assets) = Assets::new() {
        assets
    } else {
        println!("Could not find assets/ in current directory or any of its parent directories.");
        return;
    };

    let mut audio = Audio::new(assets);

    // CLI options
    let (mut menu, mut game) = {
        #[allow(unused_variables)] // Needed for headless build
        match cli_results.continue_from {
            ContinueFrom::Menu => {
                audio.play_bgm("Menu");
                (Menu::new(MenuState::GameSelect), None)
            }
            ContinueFrom::Game => {
                // handle issues with package that prevent starting from game
                if package.as_ref().unwrap().entities.len() == 0 {
                    println!("package has no entities");
                    return;
                } else if package.as_ref().unwrap().stages.len() == 0 {
                    println!("package has no stages");
                    return;
                }

                // handle missing and invalid cli input
                for name in &cli_results.fighter_names {
                    if !package.as_ref().unwrap().entities.contains_key(name) {
                        println!("Package does not contain selected fighter '{}'", name);
                        return;
                    }
                }
                if let &Some(ref name) = &cli_results.stage_name {
                    if !package.as_ref().unwrap().stages.contains_key(name) {
                        println!("Package does not contain selected stage '{}'", name);
                        return;
                    }
                }

                // handle missing and invalid cli input
                if cli_results.fighter_names.is_empty() {
                    cli_results
                        .fighter_names
                        .push(package.as_ref().unwrap().entities.index_to_key(0).unwrap());
                }

                // fill players/controllers
                let mut controllers: Vec<usize> = vec![];
                let mut players: Vec<PlayerSetup> = vec![];
                input.step(&[], &[], &mut netplay, false); // run the first input step so that we can check for the number of controllers.
                let input_len = input.players(0, &netplay).len();
                for i in 0..input_len {
                    controllers.push(i);
                    players.push(PlayerSetup {
                        fighter: cli_results.fighter_names[i % cli_results.fighter_names.len()]
                            .clone(),
                        team: i,
                    });
                }

                // remove extra players/controllers
                if let Some(max_players) = cli_results.max_human_players {
                    while controllers.len() > max_players {
                        controllers.pop();
                        players.pop();
                    }
                }

                // add cpu players
                let mut ais: Vec<usize> = vec![];
                let players_len = players.len();
                if let Some(total_players) = cli_results.total_cpu_players {
                    for i in 0..total_players {
                        players.push(PlayerSetup {
                            fighter: cli_results.fighter_names
                                [(players_len + i) % cli_results.fighter_names.len()]
                            .clone(),
                            team: players_len + i,
                        });
                        controllers.push(input_len + i);
                        ais.push(0);
                    }
                }

                if cli_results.stage_name.is_none() {
                    cli_results.stage_name = package.as_ref().unwrap().stages.index_to_key(0);
                }

                let rules = Rules {
                    time_limit_seconds: None,
                    ..Default::default()
                };

                let setup = GameSetup {
                    init_seed: GameSetup::gen_seed(),
                    input_history: vec![],
                    entity_history: Default::default(),
                    stage_history: vec![],
                    stage: cli_results.stage_name.unwrap(),
                    state: GameState::Local,
                    debug: cli_results.debug,
                    max_history_frames: cli_results.max_history_frames,
                    current_frame: 0,
                    deleted_history_frames: 0,
                    debug_entities: Default::default(),
                    debug_stage: Default::default(),
                    camera: Camera::new(),
                    edit: Edit::Stage,
                    hot_reload_entities: None,
                    hot_reload_stage: None,
                    rules,
                    controllers,
                    players,
                    ais,
                };
                (
                    Menu::new(MenuState::character_select()),
                    Some(Game::new(package.take().unwrap(), setup, &mut audio)),
                )
            }
            ContinueFrom::ReplayFile(file_name) => match replays::load_replay(&file_name) {
                Ok(replay) => {
                    let mut game_setup = replay.into_game_setup(true);
                    input.set_history(std::mem::take(&mut game_setup.input_history));
                    (
                        Menu::new(MenuState::character_select()),
                        Some(Game::new(package.take().unwrap(), game_setup, &mut audio)),
                    )
                }
                Err(err) => {
                    println!(
                        "Failed to load replay with filename '{}', because: {}",
                        file_name, err
                    );
                    return;
                }
            },
            ContinueFrom::Netplay => {
                audio.play_bgm("Menu");
                netplay.direct_connect(cli_results.address.unwrap());
                let state = MenuState::NetplayWait {
                    message: String::from(""),
                };

                (Menu::new(state), None)
            }
            ContinueFrom::MatchMaking => {
                audio.play_bgm("Menu");
                netplay.connect_match_making(
                    cli_results
                        .netplay_region
                        .unwrap_or(config.netplay_region.clone().unwrap_or_else(|| "AU".into())),
                    cli_results.netplay_players.unwrap_or(2),
                );
                let state = MenuState::NetplayWait {
                    message: String::from(""),
                };

                (Menu::new(state), None)
            }
            ContinueFrom::Close => unreachable!(),
        }
    };

    let mut command_line = CommandLine::new();
    let mut os_input = WinitInputHelper::new();
    let mut events = vec![];

    loop {
        debug!("\n\nAPP LOOP START");
        let frame_start = Instant::now();

        netplay.step();

        // TODO:
        // *    use 1/60s timer to update current_frame variable
        // *    keep rerunning the last frame as new information comes in (inputs)
        // *    if the current_frame has not yet updated then we need to reset to the previous frames state first
        //      +   needs to be for both menu + game logic or else things will break
        //      +   should this be implemented seperately for menu + game?
        events.clear();
        while let Ok(event) = event_rx.try_recv() {
            events.push(event);
        }
        os_input.step_with_window_events(&events);

        let mut resume_menu: Option<ResumeMenu> = None;
        if let Some(ref mut game) = game {
            if let NetplayState::Disconnected { reason } = netplay.state() {
                resume_menu = Some(ResumeMenu::NetplayDisconnect { reason });
            } else {
                let ai_inputs = ai::gen_inputs(game);
                let reset_deadzones = game.check_reset_deadzones();
                input.step(&game.tas, &ai_inputs, &mut netplay, reset_deadzones);

                if let GameState::Quit(resume_menu_inner) = game.step(
                    &mut config,
                    &mut input,
                    &os_input,
                    command_line.block(),
                    &netplay,
                    &mut audio,
                ) {
                    resume_menu = Some(resume_menu_inner)
                }
                if let Err(_) = render_tx.send(game.graphics_message(&config, &command_line)) {
                    return;
                }
                if let NetplayState::Offline = netplay.state() {
                    net_command_line.step(game);
                    command_line.step(&os_input, game);
                }
            }
        } else {
            input.step(&[], &[], &mut netplay, false);
            if let Some(mut menu_game_setup) = menu.step(
                package.as_ref().unwrap(),
                &mut config,
                &mut input,
                &os_input,
                &mut netplay,
            ) {
                input.set_history(std::mem::take(&mut menu_game_setup.input_history));
                game = Some(Game::new(
                    package.take().unwrap(),
                    menu_game_setup,
                    &mut audio,
                ));
            } else if let Err(_) = render_tx.send(menu.graphics_message(
                package.as_mut().unwrap(),
                &config,
                &command_line,
            )) {
                return;
            }
        }

        if let Some(resume_menu) = resume_menu {
            package = Some(game.unwrap().reclaim());

            input.reset_history();
            game = None;
            menu.resume(resume_menu, &mut audio);

            // Game -> Menu Transitions
            // Game complete   -> display results -> CSS
            // Game quit       -> CSS
            // Replay complete -> display results -> replay screen
            // Replay quit     -> replay screen
        }

        if os_input.quit() {
            netplay.set_offline(); // tell peer we are quiting
            return;
        }

        let frame_duration = Duration::from_secs(1) / 60;
        let frame_elapsed = frame_start.elapsed();
        if frame_elapsed < frame_duration {
            spin_sleep::sleep(frame_duration - frame_elapsed);
        }
    }
}
