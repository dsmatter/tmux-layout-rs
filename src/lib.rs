pub mod cli;
pub mod config;
pub mod cwd;
pub mod tmux;

use colored::Colorize;

pub fn exit_with_error(msg: &str) -> ! {
    eprintln!("{} {}", "error:".red().bold(), msg);
    std::process::exit(1)
}

pub fn show_warning(msg: &str) {
    eprintln!("{} {}", "warning:".yellow().bold(), msg);
}

pub fn show_info(msg: &str) {
    eprintln!("{} {}", "info:".green().bold(), msg);
}
