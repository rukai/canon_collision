use env_logger::fmt::{Color, Formatter};
use env_logger::Builder;
use log::{Level, Record};
use std::env;
use std::io;
use std::io::Write;

pub fn init() {
    let env_var = env::var("CC_LOG").unwrap_or("warn".into());
    Builder::new().format(format).parse_filters(&env_var).init()
}

fn format(buf: &mut Formatter, record: &Record) -> io::Result<()> {
    let level = record.level();
    let level_color = match level {
        Level::Trace => Color::White,
        Level::Debug => Color::Blue,
        Level::Info => Color::Green,
        Level::Warn => Color::Yellow,
        Level::Error => Color::Red,
    };

    let mut style = buf.style();
    style.set_color(level_color);

    let write_level = write!(buf, "{:>5}", style.value(level));
    let write_args = if let Some(module_path) = record.module_path() {
        writeln!(buf, " {} {}", module_path, record.args())
    } else {
        writeln!(buf, " {}", record.args())
    };

    write_level.and(write_args)
}
