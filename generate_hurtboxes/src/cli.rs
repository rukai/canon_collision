use getopts::Options;
use std::env;

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} -f fighter [options]", program);
    print!("{}", opts.usage(&brief));
}

pub fn cli() -> CLIResults {
    let args: Vec<String> = env::args().collect();
    let program = &args[0];

    let mut opts = Options::new();
    opts.optflag("h", "hitboxes", "Delete any existing hitboxes on the generated actions");
    opts.optflag("r", "resize", "Resize generated action length");
    opts.reqopt("f",  "fighter", "Use the fighter specified", "NAME");
    opts.optopt("a",  "actions",  "Generate hurtboxes for the actions specified",  "NAME1,NAME2,NAME3...");

    let mut results = CLIResults::new();

    let matches = match opts.parse(&args[1..]) {
        Ok(m)  => m,
        Err(_) => {
            print_usage(program, opts);
            return results;
        },
    };

    results.hitboxes = matches.opt_present("h");
    results.resize = matches.opt_present("r");
    results.fighter_name = matches.opt_str("f");

    if let Some(fighter_names) = matches.opt_str("a") {
        for fighter_name in fighter_names.split(",") {
            results.action_names.push(fighter_name.to_string());
        }
    }

    results
}

pub struct CLIResults {
    pub fighter_name: Option<String>,
    pub action_names: Vec<String>,
    pub hitboxes:     bool,
    pub resize:       bool,
}

impl CLIResults {
    pub fn new() -> CLIResults {
        CLIResults {
            fighter_name: None,
            action_names: vec!(),
            hitboxes:     false,
            resize:       false,
        }
    }
}
