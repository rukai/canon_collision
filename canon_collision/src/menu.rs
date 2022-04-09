use crate::audio::Audio;
use crate::camera::Camera;
use crate::game::{GameSetup, GameState, PlayerSetup, Edit};
use crate::graphics::{GraphicsMessage, Render, RenderType};
use crate::graphics;
use crate::replays;
use crate::results::{GameResults, PlayerResult};

use canon_collision_lib::command_line::CommandLine;
use canon_collision_lib::config::Config;
use canon_collision_lib::input::Input;
use canon_collision_lib::input::state::PlayerInput;
use canon_collision_lib::network::{Netplay, NetplayState};
use canon_collision_lib::package::Package;
use canon_collision_lib::replays_files;

use treeflection::{Node, NodeRunner, NodeToken};
use winit::event::VirtualKeyCode;
use winit_input_helper::WinitInputHelper;

use std::mem;

/// For player convenience some data is kept when moving between menus.
/// This data is stored in the Menu struct.
///
/// Because it should be refreshed (sourced from filesystem)
///     or is no longer valid (e.g. back_counter) some data is thrown away when moving between menus.
///     or takes up too much space when copied for netplay e.g. game_results
/// This data is kept in the MenuState variants.

pub struct Menu {
    state:              MenuState,
    prev_state:         Option<MenuState>, // Only populated when the current state specifically needs to jump back to the previous state i.e we could arrive at the current state via multiple sources.
    fighter_selections: Vec<PlayerSelect>,
    game_ticker:        MenuTicker,
    stage_ticker:       Option<MenuTicker>, // Uses an option because we dont know how many stages there are at Menu creation, but we want to remember which stage was selected
    current_frame:      usize,
    back_counter_max:   usize,
    game_setup:         Option<GameSetup>,
    game_results:       Option<GameResults>,
    netplay_history:    Vec<NetplayHistory>,
}

pub struct NetplayHistory {
    state:              MenuState,
    prev_state:         Option<MenuState>,
    fighter_selections: Vec<PlayerSelect>,
    stage_ticker:       Option<MenuTicker>,
}

impl Menu {
    pub fn new(state: MenuState) -> Menu {
        Menu {
            state,
            prev_state:         None,
            fighter_selections: vec!(),
            stage_ticker:       None,
            game_ticker:        MenuTicker::new(3),
            current_frame:      0,
            back_counter_max:   90,
            game_setup:         None,
            game_results:       None,
            netplay_history:    vec!(),
        }
    }

    pub fn resume(&mut self, resume_menu: ResumeMenu, audio: &mut Audio) {
        audio.play_bgm("Menu");

        self.current_frame = 0;
        match resume_menu {
            ResumeMenu::NetplayDisconnect { reason: message } => {
                self.state = MenuState::NetplayWait { message };
            }
            ResumeMenu::Results (results) => {
                self.game_results = Some(results);
                self.prev_state = Some(mem::replace(&mut self.state, MenuState::game_results()));
            }
            ResumeMenu::Unchanged => { }
        }
    }

    pub fn step_game_select(&mut self, package: &Package, config: &mut Config, player_inputs: &[PlayerInput], netplay: &mut Netplay) {
        let ticker = &mut self.game_ticker;

        if player_inputs.iter().any(|x| x[0].stick_y > 0.4 || x[0].up) {
            ticker.up();
        }
        else if player_inputs.iter().any(|x| x[0].stick_y < -0.4 || x[0].down) {
            ticker.down();
        }
        else {
            ticker.reset();
        }

        if (player_inputs.iter().any(|x| x.a.press || x.start.press)) && package.stages.len() > 0 {
            match ticker.cursor {
                0 => {
                    self.state = MenuState::character_select()
                }
                1 => {
                    netplay.connect_match_making(
                        config.netplay_region.clone().unwrap_or(String::from("AU")), // TODO: set region screen if region.is_none()
                        2
                    );
                    self.state = MenuState::NetplayWait { message: String::from("") };
                }
                2 => {
                    self.state = MenuState::replay_select();
                }
                _ => unreachable!()
            }
        }
    }

    pub fn step_replay_select(&mut self, player_inputs: &[PlayerInput]) {
        let back = if let &mut MenuState::ReplaySelect (ref replays, ref mut ticker) = &mut self.state {
            if player_inputs.iter().any(|x| x[0].stick_y > 0.4 || x[0].up) {
                ticker.up();
            }
            else if player_inputs.iter().any(|x| x[0].stick_y < -0.4 || x[0].down) {
                ticker.down();
            }
            else {
                ticker.reset();
            }

            if (player_inputs.iter().any(|x| x.start.press || x.a.press)) && !replays.is_empty() {
                let name = &replays[ticker.cursor];
                match replays::load_replay(&format!("{}.zip", name)) {
                    Ok(replay) => {
                        self.game_setup = Some(replay.into_game_setup(false));
                    }
                    Err(error) => {
                        println!("Failed to load replay: {}\n{}", name, error);
                    }
                }
                false
            }
            else {
                player_inputs.iter().any(|x| x.b.press)
            }
        } else { unreachable!() };

        if back {
            self.state = MenuState::GameSelect;
        }
    }

    /// If controllers are added or removed then the indexes
    /// are going be out of whack so just reset the fighter selection state
    /// If a controller is added on the same frame another is removed, then no reset occurs.
    /// However this is rare and the problem is minor, so ¯\_(ツ)_/¯
    fn add_remove_fighter_selections(&mut self, package: &Package, player_inputs: &[PlayerInput]) {
        if self.fighter_selections.iter().filter(|x| !x.ui.is_cpu()).count() != player_inputs.len()
        {
            self.fighter_selections.clear();
            for (i, input) in player_inputs.iter().enumerate() {
                let ui = if input.plugged_in {
                    PlayerSelectUi::human_fighter(package)
                } else {
                    PlayerSelectUi::HumanUnplugged
                };
                let team = Menu::get_free_team(&self.fighter_selections);
                self.fighter_selections.push(PlayerSelect {
                    controller:      Some((i, MenuTicker::new(1))),
                    fighter:         None,
                    cpu_ai:          None,
                    ui,
                    animation_frame: 0,
                    team
                });
            }
        }
    }

    fn step_fighter_select(&mut self, package: &Package, player_inputs: &[PlayerInput], netplay: &mut Netplay) {
        self.add_remove_fighter_selections(package, player_inputs);
        let fighters = package.fighters();

        let mut new_state: Option<MenuState> = None;
        if let &mut MenuState::CharacterSelect { ref mut back_counter } = &mut self.state {
            // animate fighters
            for selection in self.fighter_selections.iter_mut() {
                if let Some(fighter_key) = selection.fighter {
                    let fighter = &fighters[fighter_key].1;
                    if fighter.actions.contains_key(&fighter.css_action) {
                        let action = &fighter.actions[fighter.css_action.as_ref()];
                        selection.animation_frame = (selection.animation_frame + 1) % action.frames.len();
                    }
                }
            }

            // plug/unplug humans
            for (input_i, input) in player_inputs.iter().enumerate() {
                let free_team = Menu::get_free_team(&self.fighter_selections);
                if input.plugged_in {
                    let selection = &mut self.fighter_selections[input_i];
                    if let PlayerSelectUi::HumanUnplugged = selection.ui {
                        selection.ui = PlayerSelectUi::human_fighter(package);
                        selection.team = free_team;
                        selection.controller = Some((input_i, MenuTicker::new(1)));
                    }
                }
                else if let PlayerSelectUi::HumanFighter (_) = self.fighter_selections[input_i].ui {
                    self.fighter_selections[input_i].ui = PlayerSelectUi::HumanUnplugged;

                    // Handle CPU's who are currently manipulated by the input
                    for selection in &mut self.fighter_selections {
                        if let Some((controller, _)) = selection.controller.clone() {
                            if controller == input_i {
                                selection.controller = None
                            }
                        }
                    }
                }
            }

            for (controller_i, input) in player_inputs.iter().enumerate() {
                if !input.plugged_in {
                    continue;
                }

                // get current selection
                let mut selection_i = 0;
                for (check_selection_i, selection) in self.fighter_selections.iter().enumerate() {
                    if let Some((check_controller_i, _)) = selection.controller {
                        if check_controller_i == controller_i {
                            selection_i = check_selection_i;
                        }
                    }
                }

                // move left/right
                if input[0].stick_x < -0.7 || input[0].left {
                    if self.fighter_selections[selection_i].controller.as_mut().unwrap().1.tick() {
                        // find prev selection to move to
                        let mut new_selection_i: Option<usize> = None;
                        for (check_selection_i, selection) in self.fighter_selections.iter().enumerate() {
                            if check_selection_i > selection_i && (selection.is_free() || check_selection_i == controller_i) {
                                new_selection_i = Some(check_selection_i);
                            }
                        }
                        for (check_selection_i, selection) in self.fighter_selections.iter().enumerate() {
                            if check_selection_i < selection_i && (selection.is_free() || check_selection_i == controller_i) {
                                new_selection_i = Some(check_selection_i);
                            }
                        }

                        // move selection
                        if let Some(new_selection_i) = new_selection_i {
                            self.fighter_selections[new_selection_i].controller = self.fighter_selections[selection_i].controller.clone();
                            self.fighter_selections[selection_i].controller = None;
                            self.fighter_selections[selection_i].ui.ticker_full_reset();
                        }
                    }
                }
                else if input[0].stick_x > 0.7 || input[0].right {
                    if self.fighter_selections[selection_i].controller.as_mut().unwrap().1.tick() {
                        // find next selection to move to
                        let mut new_selection_i: Option<usize> = None;
                        for (check_selection_i, selection) in self.fighter_selections.iter().enumerate().rev() {
                            if check_selection_i < selection_i && (selection.is_free() || check_selection_i == controller_i) {
                                new_selection_i = Some(check_selection_i);
                            }
                        }
                        for (check_selection_i, selection) in self.fighter_selections.iter().enumerate().rev() {
                            if check_selection_i > selection_i && (selection.is_free() || check_selection_i == controller_i) {
                                new_selection_i = Some(check_selection_i);
                            }
                        }

                        // move selection
                        if let Some(new_selection_i) = new_selection_i {
                            self.fighter_selections[new_selection_i].controller = self.fighter_selections[selection_i].controller.clone();
                            self.fighter_selections[selection_i].controller = None;
                            self.fighter_selections[selection_i].ui.ticker_full_reset();
                        }
                    }
                }
                else {
                    self.fighter_selections[selection_i].controller.as_mut().unwrap().1.reset();
                }
            }

            // update selections
            let mut add_cpu = false;
            let mut remove_cpu: Option<usize> = None;

            for (selection_i, selection) in self.fighter_selections.iter_mut().enumerate() {
                if let Some((controller, _)) = selection.controller {
                    let input = &player_inputs[controller];
                    if input.b.press {
                        match selection.ui.clone() {
                            PlayerSelectUi::HumanFighter (_) |
                            PlayerSelectUi::CpuFighter (_) => {
                                selection.fighter = None;
                            }
                            PlayerSelectUi::HumanTeam (_) => {
                                selection.ui = PlayerSelectUi::human_fighter(package);
                            }
                            PlayerSelectUi::CpuTeam (_) |
                            PlayerSelectUi::CpuAi (_) => {
                                selection.ui = PlayerSelectUi::cpu_fighter(package);
                                selection.ui = PlayerSelectUi::cpu_fighter(package);
                            }
                            PlayerSelectUi::HumanUnplugged => unreachable!(),
                        }
                    }
                    else if input.a.press {
                        match selection.ui.clone() {
                            PlayerSelectUi::HumanFighter (ticker) => {
                                if ticker.cursor < fighters.len() {
                                    selection.fighter = Some(ticker.cursor);
                                    selection.animation_frame = 0;
                                }
                                else {
                                    match ticker.cursor - fighters.len() {
                                        0 => { selection.ui = PlayerSelectUi::human_team() }
                                        1 => { add_cpu = true; }
                                        _ => { unreachable!() }
                                    }
                                }
                            }
                            PlayerSelectUi::CpuFighter (ticker) => {
                                if ticker.cursor < fighters.len() {
                                    selection.fighter = Some(ticker.cursor);
                                    selection.animation_frame = 0;
                                }
                                else {
                                    match ticker.cursor - fighters.len() {
                                        0 => { selection.ui = PlayerSelectUi::cpu_team() }
                                        1 => { /* TODO: selection.ui = PlayerSelectUi::cpu_ai()*/ }
                                        2 => { remove_cpu = Some(selection_i); }
                                        _ => { unreachable!() }
                                    }
                                }
                            }
                            PlayerSelectUi::HumanTeam (ticker) => {
                                let colors = graphics::get_colors();
                                if ticker.cursor < colors.len() {
                                    selection.team = ticker.cursor;
                                } else {
                                    match ticker.cursor - colors.len() {
                                        0 => { selection.ui = PlayerSelectUi::human_fighter(package) }
                                        _ => { unreachable!() }
                                    }
                                }
                            }
                            PlayerSelectUi::CpuTeam (ticker) => {
                                let colors = graphics::get_colors();
                                if ticker.cursor < colors.len() {
                                    selection.team = ticker.cursor;
                                } else {
                                    match ticker.cursor - colors.len() {
                                        0 => { selection.ui = PlayerSelectUi::cpu_fighter(package) }
                                        _ => { unreachable!() }
                                    }
                                }
                            }
                            PlayerSelectUi::CpuAi (_) => { }
                            PlayerSelectUi::HumanUnplugged => unreachable!(),
                        }
                    }

                    match selection.ui {
                        PlayerSelectUi::HumanFighter (ref mut ticker) |
                        PlayerSelectUi::CpuFighter   (ref mut ticker) |
                        PlayerSelectUi::HumanTeam    (ref mut ticker) |
                        PlayerSelectUi::CpuTeam      (ref mut ticker) |
                        PlayerSelectUi::CpuAi        (ref mut ticker) => {
                            if input[0].stick_y > 0.4 || input[0].up {
                                ticker.up();
                            }
                            else if input[0].stick_y < -0.4 || input[0].down {
                                ticker.down();
                            }
                            else {
                                ticker.reset();
                            }
                        }
                        PlayerSelectUi::HumanUnplugged => { }
                    }
                }
            }

            // run selection modifications that were previously immutably borrowed
            if let Some(selection_i) = remove_cpu {
                let home_selection_i = self.fighter_selections[selection_i].controller.clone().unwrap().0;
                self.fighter_selections[home_selection_i].controller = self.fighter_selections[selection_i].controller.clone();
                self.fighter_selections.remove(selection_i);
            }

            if add_cpu && self.fighter_selections.iter().filter(|x| x.ui.is_visible()).count() < 4 {
                let team = Menu::get_free_team(&self.fighter_selections);
                self.fighter_selections.push(PlayerSelect {
                    controller:      None,
                    fighter:         None,
                    cpu_ai:          None,
                    ui:              PlayerSelectUi::cpu_fighter(package),
                    animation_frame: 0,
                    team
                });
            }

            if player_inputs.iter().any(|x| x.start.press) && !fighters.is_empty() {
                new_state = Some(MenuState::StageSelect);
                if self.stage_ticker.is_none() {
                    self.stage_ticker = Some(MenuTicker::new(package.stages.len()));
                }
            }
            else if player_inputs.iter().any(|x| x[0].b) {
                if *back_counter > self.back_counter_max {
                    netplay.set_offline();
                    new_state = Some(MenuState::GameSelect);
                }
                else {
                    *back_counter += 1;
                }
            }
            else {
                *back_counter = 0;
            }
        }

        if let Some(state) = new_state {
            self.state = state;
        }
    }

    fn get_free_team(selections: &[PlayerSelect]) -> usize {
        let mut team = 0;
        while selections.iter().any(|x| x.ui.is_visible() && x.team == team) {
            team += 1;
        }
        team
    }

    fn step_stage_select(&mut self, package: &Package, player_inputs: &[PlayerInput], netplay: &Netplay) {
        if self.stage_ticker.is_none() {
            self.stage_ticker = Some(MenuTicker::new(package.stages.len()));
        }

        let ticker = self.stage_ticker.as_mut().unwrap();

        if player_inputs.iter().any(|x| x[0].stick_y > 0.4 || x[0].up) {
            ticker.up();
        }
        else if player_inputs.iter().any(|x| x[0].stick_y < -0.4 || x[0].down) {
            ticker.down();
        }
        else {
            ticker.reset();
        }

        if (player_inputs.iter().any(|x| x.start.press || x.a.press)) && package.stages.len() > 0 {
            self.game_setup(package, netplay);
        }
        else if player_inputs.iter().any(|x| x.b.press) {
            self.state = MenuState::character_select();
        }
    }

    pub fn game_setup(&mut self, package: &Package, netplay: &Netplay) {
        let mut players: Vec<PlayerSetup> = vec!();
        let mut controllers: Vec<usize> = vec!();
        let mut ais: Vec<usize> = vec!();
        let mut ais_skipped = 0;
        let fighters = package.fighters();
        for (i, selection) in (&self.fighter_selections).iter().enumerate() {
            // add human players
            if selection.ui.is_human_plugged_in() {
                if let Some(fighter) = selection.fighter {
                    players.push(PlayerSetup {
                        fighter: fighters[fighter].0.clone(),
                        team:    selection.team,
                    });
                    controllers.push(i);
                }
            }

            // add CPU players
            if selection.ui.is_cpu() {
                if selection.fighter.is_some() /* && selection.cpu.is_some() TODO */ {
                    let fighter = selection.fighter.unwrap();
                    players.push(PlayerSetup {
                        fighter: fighters[fighter].0.clone(),
                        team:    selection.team,
                    });
                    controllers.push(i - ais_skipped);
                    ais.push(0); // TODO: delete this
                    // ais.push(selection.cpu_ai.unwrap()); TODO: add this
                }
                else {
                    ais_skipped += 1;
                }
            }
        }

        let stage = package.stages.index_to_key(self.stage_ticker.as_ref().unwrap().cursor).unwrap();
        let state = if netplay.number_of_peers() == 1 { GameState::Local } else { GameState::Netplay };
        let init_seed = netplay.get_seed().unwrap_or(GameSetup::gen_seed());

        self.game_setup = Some(GameSetup {
            input_history:          vec!(),
            entity_history:         Default::default(),
            stage_history:          vec!(),
            rules:                  Default::default(), // TODO: this will be configured by the user in the menu
            debug:                  false,
            max_history_frames:     None,
            current_frame:          0,
            deleted_history_frames: 0,
            debug_entities:         Default::default(),
            debug_stage:            Default::default(),
            camera:                 Camera::new(),
            edit:                   Edit::Stage,
            hot_reload_entities:    None,
            hot_reload_stage:       None,
            init_seed,
            controllers,
            ais,
            players,
            stage,
            state,
        });
    }

    fn step_results(&mut self, config: &Config, player_inputs: &[PlayerInput]) {
        if player_inputs.iter().any(|x| x.start.press || x.a.press) {
            self.state = self.prev_state.take().unwrap();
        }

        // TODO:
        // Make the following changes so this state is managed locally (one peer saving a replay does not cause the other to save a replay)
        // *    Run it in a seperate thread so main thread does not halt
        // *    Dont use remote peers inputs
        // *    move replay_saved into its own non-rollbacked state
        if let &mut MenuState::GameResults { ref mut replay_saved, .. } = &mut self.state {
            if !*replay_saved && (config.auto_save_replay || player_inputs.iter().any(|x| x.l.press && x.r.press)) {
                replays::save_replay(&self.game_results.as_ref().unwrap().replay);
                *replay_saved = true;
            }
        }
    }

    fn step_netplay_wait(&mut self, player_inputs: &[PlayerInput], netplay: &mut Netplay) {
        if player_inputs.iter().any(|x| x.b.press) {
            self.state = MenuState::GameSelect;
        }

        let loading_characters = ["|", "/", "-", "\\"];
        let load_character = loading_characters[(self.current_frame / 5) % loading_characters.len()];

        match netplay.state() {
            NetplayState::Offline => { }
            NetplayState::MatchMaking { request, .. } => {
                self.state = MenuState::NetplayWait { message: format!("Searching for online match in {} {}", request.region, load_character) };
                if player_inputs.iter().any(|x| x.b.press) {
                    netplay.set_offline();
                    self.state = MenuState::GameSelect;
                }
            }
            NetplayState::InitConnection {..} => {
                self.state = MenuState::NetplayWait { message: format!("Connecting to peer {}", load_character) };
                if player_inputs.iter().any(|x| x.b.press) {
                    netplay.set_offline();
                    self.state = MenuState::GameSelect;
                }
            }
            NetplayState::PingTest { .. } => {
                self.state = MenuState::NetplayWait { message: format!("Testing ping {}", load_character) };
                if player_inputs.iter().any(|x| x.b.press) {
                    netplay.set_offline();
                    self.state = MenuState::GameSelect;
                }
            }
            NetplayState::Disconnected { .. } => {
                if player_inputs.iter().any(|x| x.a.press || x.b.press) {
                    netplay.set_offline();
                    self.state = MenuState::GameSelect;
                }
            }
            NetplayState::Running { .. } => {
                self.state = MenuState::character_select();
            }
        }
    }

    pub fn step(&mut self, package: &Package, config: &mut Config, input: &mut Input, os_input: &WinitInputHelper, netplay: &mut Netplay) -> Option<GameSetup> {
        if os_input.held_alt() && os_input.key_pressed(VirtualKeyCode::Return) {
            config.fullscreen = !config.fullscreen;
            config.save();
        }

        // skip a frame so the other clients can catch up.
        if !netplay.skip_frame() {
            self.current_frame += 1;

            let start = self.current_frame - netplay.frames_to_step();
            let end = self.current_frame;

            self.netplay_history.truncate(start);
            if start > 0 {
                let history = self.netplay_history.get(start-1).unwrap();
                self.state              = history.state.clone();
                self.prev_state         = history.prev_state.clone();
                self.fighter_selections = history.fighter_selections.clone();
                self.stage_ticker       = history.stage_ticker.clone();
            }

            input.netplay_update();

            for frame in start..end {
                if let NetplayState::Disconnected { reason } = netplay.state() {
                    self.state = MenuState::NetplayWait { message: reason };
                }

                let player_inputs = input.players(frame, netplay);

                // In order to avoid hitting buttons still held down from the game, dont do anything on the first frame.
                if frame > 1 {
                    match self.state {
                        MenuState::GameSelect           => self.step_game_select   (package, config, &player_inputs, netplay),
                        MenuState::ReplaySelect (_, _)  => self.step_replay_select (&player_inputs),
                        MenuState::CharacterSelect {..} => self.step_fighter_select(package, &player_inputs, netplay),
                        MenuState::StageSelect          => self.step_stage_select  (package, &player_inputs, netplay),
                        MenuState::GameResults {..}     => self.step_results       (config, &player_inputs),
                        MenuState::NetplayWait {..}     => self.step_netplay_wait  (&player_inputs, netplay),
                    };
                }

                self.netplay_history.push(NetplayHistory {
                    state:              self.state.clone(),
                    prev_state:         self.prev_state.clone(),
                    fighter_selections: self.fighter_selections.clone(),
                    stage_ticker:       self.stage_ticker.clone(),
                });
            }
        }

        debug!("current_frame: {}", self.current_frame);
        self.game_setup.take()
    }

    #[allow(dead_code)] // Needed for headless build
    pub fn render(&self) -> RenderMenu {
        RenderMenu {
            state: match self.state {
                MenuState::GameResults { replay_saved } => RenderMenuState::GameResults { results: self.game_results.as_ref().unwrap().player_results.clone(), replay_saved },
                MenuState::CharacterSelect { back_counter, .. } => RenderMenuState::CharacterSelect (self.fighter_selections.clone(), back_counter, self.back_counter_max),
                MenuState::ReplaySelect (ref replays, ref ticker) => RenderMenuState::ReplaySelect (replays.clone(), ticker.cursor),
                MenuState::NetplayWait { ref message } => RenderMenuState::GenericText (message.clone()),
                MenuState::GameSelect  => RenderMenuState::GameSelect  (self.game_ticker.cursor),
                MenuState::StageSelect => RenderMenuState::StageSelect (self.stage_ticker.as_ref().unwrap().cursor),
            },
        }
    }

    #[allow(dead_code)] // Needed for headless build
    pub fn graphics_message(&mut self, package: &mut Package, config: &Config, command_line: &CommandLine) -> GraphicsMessage {
        let updates = package.updates();

        let render = Render {
            command_output:  command_line.output(),
            render_type:     RenderType::Menu (self.render()),
            fullscreen:      config.fullscreen
        };

        GraphicsMessage {
            package_updates: updates,
            render,
        }
    }
}

#[derive(Clone)]
pub enum MenuState {
    GameSelect,
    ReplaySelect (Vec<String>, MenuTicker), // MenuTicker must be tied with the Vec<String>, otherwise they may become out of sync
    CharacterSelect { back_counter: usize },
    StageSelect,
    GameResults { replay_saved: bool },
    NetplayWait { message: String },
}

impl MenuState {
    pub fn replay_select() -> MenuState {
        let replays = replays_files::get_replay_names();
        let ticker = MenuTicker::new(replays.len());
        MenuState::ReplaySelect (replays, ticker)
    }

    pub fn character_select() -> MenuState {
        MenuState::CharacterSelect { back_counter: 0 }
    }

    pub fn game_results() -> MenuState {
        MenuState::GameResults { replay_saved: false }
    }
}

pub enum RenderMenuState {
    GameSelect      (usize),
    ReplaySelect    (Vec<String>, usize),
    CharacterSelect (Vec<PlayerSelect>, usize, usize),
    StageSelect     (usize),
    GameResults     { results: Vec<PlayerResult>, replay_saved: bool },
    GenericText     (String),
}

#[derive(Clone)]
pub struct PlayerSelect {
    pub controller:      Option<(usize, MenuTicker)>, // the cursor of the ticker is ignored
    pub fighter:         Option<usize>,
    pub cpu_ai:          Option<usize>,
    pub team:            usize,
    pub ui:              PlayerSelectUi,
    pub animation_frame: usize,
}

impl PlayerSelect {
    /// Returns true iff a controller can move to this selection
    pub fn is_free(&self) -> bool {
        self.ui.is_cpu() && self.controller.is_none()
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub enum PlayerSelectUi {
    CpuAi        (MenuTicker),
    CpuFighter   (MenuTicker),
    CpuTeam      (MenuTicker),
    HumanFighter (MenuTicker),
    HumanTeam    (MenuTicker),
    HumanUnplugged,
}

impl PlayerSelectUi {
    #[allow(dead_code)]
    pub fn cpu_ai() -> Self {
        PlayerSelectUi::CpuAi (MenuTicker::new(/* TODO: number_of_ai + */ 1))
    }

    pub fn cpu_fighter(package: &Package) -> Self {
        PlayerSelectUi::CpuFighter (MenuTicker::new(package.fighters().len() + 3))
    }

    pub fn human_fighter(package: &Package) -> Self {
        PlayerSelectUi::HumanFighter (MenuTicker::new(package.fighters().len() + 2))
    }

    pub fn cpu_team() -> Self {
        PlayerSelectUi::CpuTeam (MenuTicker::new(graphics::get_colors().len() + 1))
    }

    pub fn human_team() -> Self {
        PlayerSelectUi::HumanTeam (MenuTicker::new(graphics::get_colors().len() + 1))
    }

    pub fn is_visible(&self) -> bool {
        match self {
            &PlayerSelectUi::HumanUnplugged => false,
            _                               => true
        }
    }

    pub fn is_cpu(&self) -> bool {
        match self {
            &PlayerSelectUi::CpuAi (_) |
            &PlayerSelectUi::CpuFighter (_) |
            &PlayerSelectUi::CpuTeam (_) => true,
            _                            => false
        }
    }

    pub fn is_human_plugged_in(&self) -> bool {
        match self {
            &PlayerSelectUi::HumanFighter (_) |
            &PlayerSelectUi::HumanTeam (_) => true,
            _                              => false
        }
    }

    #[allow(dead_code)] // Needed for headless build
    pub fn ticker_unwrap(&self) -> &MenuTicker {
        match self {
            &PlayerSelectUi::HumanFighter (ref ticker) |
            &PlayerSelectUi::CpuFighter   (ref ticker) |
            &PlayerSelectUi::HumanTeam    (ref ticker) |
            &PlayerSelectUi::CpuTeam      (ref ticker) |
            &PlayerSelectUi::CpuAi        (ref ticker) => { ticker }
            &PlayerSelectUi::HumanUnplugged => {
                panic!("Tried to unwrap the PlayerSelectUi ticker but was HumanUnplugged")
            }
        }
    }

    pub fn ticker_full_reset(&mut self) {
        match self {
            &mut PlayerSelectUi::HumanFighter (ref mut ticker) |
            &mut PlayerSelectUi::CpuFighter   (ref mut ticker) |
            &mut PlayerSelectUi::HumanTeam    (ref mut ticker) |
            &mut PlayerSelectUi::CpuTeam      (ref mut ticker) |
            &mut PlayerSelectUi::CpuAi        (ref mut ticker) => {
                ticker.reset();
                ticker.cursor = 0;
            }
            &mut PlayerSelectUi::HumanUnplugged => { }
        }
    }
}

#[derive(Clone)]
pub struct MenuTicker {
    pub cursor:      usize,
    cursor_max:      usize,
    ticks_remaining: usize,
    tick_duration_i: usize,
    reset:           bool,
}

impl MenuTicker {
    fn new(item_count: usize) -> MenuTicker {
        MenuTicker {
            cursor:          0,
            cursor_max:      if item_count > 0 { item_count - 1 } else { 0 },
            ticks_remaining: 0,
            tick_duration_i: 0,
            reset:           true,
        }
    }

    /// increments internal state and returns true if a tick occurs
    pub fn tick(&mut self) -> bool {
        let tick_durations = [20, 12, 10, 8, 6, 5];
        if self.reset {
            self.ticks_remaining = tick_durations[0];
            self.tick_duration_i = 0;
            self.reset = false;
            true
        }

        else {
            self.ticks_remaining -= 1;
            if self.ticks_remaining == 0 {
                self.ticks_remaining = tick_durations[self.tick_duration_i];
                if self.tick_duration_i < tick_durations.len() - 1 {
                    self.tick_duration_i += 1;
                }
                true
            } else {
                false
            }
        }
    }

    fn up(&mut self) {
        if self.tick() {
            if self.cursor == 0 {
                self.cursor = self.cursor_max;
            }
            else {
                self.cursor -= 1;
            }
        }
    }

    fn down(&mut self) {
        if self.tick() {
            if self.cursor == self.cursor_max {
                self.cursor = 0;
            }
            else {
                self.cursor += 1;
            }
        }
    }

    fn reset(&mut self) {
        self.reset = true;
    }
}

pub struct RenderMenu {
    pub state: RenderMenuState,
}

/// # Game -> Menu Transitions
/// Results:   Game complete   -> display results -> CSS
/// Unchanged: Game quit       -> CSS
/// Results:   Replay complete -> display results -> replay ui
/// Unchanged: Replay quit     -> replay ui

#[derive(Clone, Serialize, Deserialize, Node)]
pub enum ResumeMenu {
    Results(GameResults),
    Unchanged,
    NetplayDisconnect { reason: String },
}

impl Default for ResumeMenu {
    fn default() -> Self {
        ResumeMenu::Unchanged
    }
}
