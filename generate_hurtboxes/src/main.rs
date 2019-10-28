mod cli;

use canon_collision_lib::package::Package;
use canon_collision_lib::fighter::ActionDef;
use cli::CLIResults;

fn main() {
    let cli = cli::cli();

    if let Some(fighter) = &cli.fighter_name {
        let mut package = if let Some(path) = Package::find_package_in_parent_dirs() {
            if let Some(package) = Package::open(path) {
                package
            } else {
                println!("Could not load package");
                return;
            }
        }
        else {
            println!("Could not find package/ in current directory or any of its parent directories.");
            return;
        };

        if let Some(ref mut fighter) = package.fighters.key_to_value_mut(&fighter) {
            for (i, ref mut action) in (*fighter.actions).iter_mut().enumerate() {
                let key = String::from("TODO"); // TODO: Get from index -> enum -> string
                if cli.action_names.len() == 0 || cli.action_names.contains(&key) {
                    regenerate_action(action, &cli);
                }
            }

            package.save();
        }
        else {
            println!("Package does not contain fighter: {}", fighter);
        }
    }
}

fn regenerate_action(action: &mut ActionDef, cli: &CLIResults) {
    println!("Hello, world!");
}
