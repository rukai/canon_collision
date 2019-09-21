#[cfg(feature = "wgpu_renderer")]
use crate::wgpu::WgpuGraphics;
#[cfg(any(feature = "wgpu_renderer"))]
use crate::cli::GraphicsBackendChoice;
#[cfg(any(feature = "wgpu_renderer"))]
use crate::graphics::GraphicsMessage;
use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc;
use std;

use canon_collision_lib::command_line::CommandLine;
use canon_collision_lib::config::Config;
use canon_collision_lib::network::{NetCommandLine, Netplay, NetplayState};
use canon_collision_lib::package::Package;
use canon_collision_lib::package;
use crate::ai;
use crate::cli::{self, ContinueFrom};
use crate::game::{Game, GameState, GameSetup, PlayerSetup};
use crate::input::Input;
use crate::menu::{Menu, MenuState, ResumeMenu};
use crate::assets::Assets;

use winit::event::Event;
use winit_input_helper::WinitInputHelper;
use libusb::Context;
use std::time::{Duration, Instant};

#[allow(unused)] // Needed for headless build
struct OsInput {
    input: WinitInputHelper<()>,
    rx: Receiver<Event<()>>
}

impl OsInput {
    fn new() -> (OsInput, Sender<Event<()>>) {
        let input = WinitInputHelper::new();
        let (tx, rx) = mpsc::channel();
        let os_input = OsInput { input, rx };
        (os_input, tx)
    }

    fn step(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            self.input.update(event); // returned bool is useless as we filter out EventsCleared
        }

        // force the WinitInputHelper to process the received events
        self.input.update(Event::EventsCleared);
    }
}

pub fn run() {
    let mut cli_results = cli::cli();
    let mut config = Config::load();
    if let ContinueFrom::Close = cli_results.continue_from {
        return;
    }

    let mut _assets = Assets::new();
    //assets.get_model("beatmesa");
    //let _foo = assets.models_reloads();

    let mut context = Context::new().unwrap();
    let mut input = Input::new(&mut context);
    #[cfg(any(feature = "wgpu_renderer"))]
    let mut graphics_tx: Option<Sender<GraphicsMessage>> = None;
    let mut net_command_line = NetCommandLine::new();
    let mut netplay = Netplay::new();

    let mut package = if let Some(package) = Package::open_or_generate("TODO") {
        Some(package)
    } else {
        println!("Could not load package");
        return;
    };

    // CLI options
    let (mut menu, mut game, mut os_input) = {
        #[allow(unused_variables)] // Needed for headless build
        let (os_input, os_input_tx) = OsInput::new();

        package::generate_example_stub();

        #[cfg(any(feature = "wgpu_renderer"))]
        {
            let physical_device_name = config.physical_device_name.clone();
            match cli_results.graphics_backend {
                #[cfg(feature = "wgpu_renderer")]
                GraphicsBackendChoice::Wgpu => {
                    graphics_tx = Some(WgpuGraphics::init(os_input_tx.clone(), physical_device_name));
                }
                GraphicsBackendChoice::Headless => {}
                GraphicsBackendChoice::Default => {
                    #[cfg(feature = "wgpu_renderer")]
                    {
                        graphics_tx = Some(WgpuGraphics::init(os_input_tx.clone(), physical_device_name));
                    }
                }
            }
        }

        match cli_results.continue_from {
            ContinueFrom::Menu => {
                (
                    Menu::new(MenuState::GameSelect),
                    None,
                    os_input
                )
            }
            ContinueFrom::Game => {
                // handle issues with package that prevent starting from game
                if package.as_ref().unwrap().fighters.len() == 0 {
                    println!("package has no fighters");
                    return;
                }
                else if package.as_ref().unwrap().stages.len() == 0 {
                    println!("package has no stages");
                    return;
                }

                // handle missing and invalid cli input
                for name in &cli_results.fighter_names {
                    if !package.as_ref().unwrap().fighters.contains_key(name) {
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
                if cli_results.fighter_names.len() == 0 {
                    cli_results.fighter_names.push(package.as_ref().unwrap().fighters.index_to_key(0).unwrap());
                }

                // fill players/controllers
                let mut controllers: Vec<usize> = vec!();
                let mut players: Vec<PlayerSetup> = vec!();
                input.step(&[], &[], &mut netplay, false); // run the first input step so that we can check for the number of controllers.
                let input_len = input.players(0, &netplay).len();
                for i in 0..input_len {
                    controllers.push(i);
                    players.push(PlayerSetup {
                        fighter: cli_results.fighter_names[i % cli_results.fighter_names.len()].clone(),
                        team:    i
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
                let mut ais: Vec<usize> = vec!();
                let players_len = players.len();
                if let Some(total_players) = cli_results.total_cpu_players {
                    for i in 0..total_players {
                        players.push(PlayerSetup {
                            fighter: cli_results.fighter_names[(players_len + i) % cli_results.fighter_names.len()].clone(),
                            team:    players_len + i
                        });
                        controllers.push(input_len + i);
                        ais.push(0);
                    }
                }

                if cli_results.stage_name.is_none() {
                    cli_results.stage_name = package.as_ref().unwrap().stages.index_to_key(0);
                }

                let setup = GameSetup {
                    init_seed:      GameSetup::gen_seed(),
                    input_history:  vec!(),
                    player_history: vec!(),
                    stage_history:  vec!(),
                    stage:          cli_results.stage_name.unwrap(),
                    state:          GameState::Local,
                    controllers,
                    players,
                    ais,
                };
                (
                    Menu::new(MenuState::character_select()),
                    Some(Game::new(package.take().unwrap(), setup)),
                    os_input
                )
            }
            ContinueFrom::Netplay => {
                netplay.direct_connect(cli_results.address.unwrap(), package.as_ref().unwrap().compute_hash());
                let state = MenuState::NetplayWait { message: String::from("") };

                (
                    Menu::new(state),
                    None,
                    os_input,
                )
            }
            ContinueFrom::MatchMaking => {
                netplay.connect_match_making(
                    cli_results.netplay_region.unwrap_or(config.netplay_region.clone().unwrap_or(String::from("AU"))),
                    cli_results.netplay_players.unwrap_or(2),
                    package.as_ref().unwrap().compute_hash()
                );
                let state = MenuState::NetplayWait { message: String::from("") };

                (
                    Menu::new(state),
                    None,
                    os_input,
                )
            }
            ContinueFrom::Close => unreachable!()
        }
    };

    let mut command_line = CommandLine::new();

    loop {
        debug!("\n\nAPP LOOP START");
        let frame_start = Instant::now();

        os_input.step();
        netplay.step();

        let mut resume_menu: Option<ResumeMenu> = None;
        if let Some(ref mut game) = game {
            if let NetplayState::Disconnected { reason } = netplay.state() {
                resume_menu = Some(ResumeMenu::NetplayDisconnect { reason });
            } else {
                let ai_inputs = ai::gen_inputs(&game);
                let reset_deadzones = game.check_reset_deadzones();
                input.step(&game.tas, &ai_inputs, &mut netplay, reset_deadzones);

                if let GameState::Quit (resume_menu_inner) = game.step(&mut config, &mut input, &os_input.input, command_line.block(), &netplay) {
                    resume_menu = Some(resume_menu_inner)
                }
                #[cfg(any(feature = "wgpu_renderer"))]
                {
                    if let Some(ref tx) = graphics_tx {
                        if let Err(_) = tx.send(game.graphics_message(&config, &command_line)) {
                            return;
                        }
                    }
                }
                if let NetplayState::Offline = netplay.state() {
                    net_command_line.step(game);
                    command_line.step(&os_input.input, game);
                }
            }
        }
        else {
            input.step(&[], &[], &mut netplay, false);
            if let Some(mut menu_game_setup) = menu.step(package.as_ref().unwrap(), &mut config, &mut input, &os_input.input, &mut netplay) {
                input.set_history(std::mem::replace(&mut menu_game_setup.input_history, vec!()));
                game = Some(Game::new(package.take().unwrap(), menu_game_setup));
            }
            else {
                #[cfg(any(feature = "wgpu_renderer"))]
                {
                    if let Some(ref tx) = graphics_tx {
                        if let Err(_) = tx.send(menu.graphics_message(package.as_mut().unwrap(), &config, &command_line)) {
                            return;
                        }
                    }
                }
            }
        }

        if let Some(resume_menu) = resume_menu {
            package = Some(game.unwrap().reclaim());

            input.reset_history();
            game = None;
            menu.resume(resume_menu);

            // Game -> Menu Transitions
            // Game complete   -> display results -> CSS
            // Game quit       -> CSS
            // Replay complete -> display results -> replay screen
            // Replay quit     -> replay screen
        }

        if os_input.input.quit() {
            netplay.set_offline(); // tell peer we are quiting
            return;
        }

        let frame_duration = Duration::from_secs(1) / 60;
        while frame_start.elapsed() < frame_duration { }
    }
}
