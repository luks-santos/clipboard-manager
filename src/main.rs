// #![allow(dead_code)]
// #![allow(unused_macros)]
// #![allow(unused_imports)]

use app::{AppState, Flags};
use config::{CONFIG_VERSION, Config};
use cosmic::cosmic_config;
use cosmic::cosmic_config::CosmicConfigEntry;
use remote::RemoteCommand;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

mod app;
mod clipboard;
mod clipboard_watcher;
mod config;
mod db;
mod icon;
mod localize;
mod message;
mod my_widget;
mod navigation;
mod remote;
mod utils;
mod view;

#[allow(unused_imports)]
#[macro_use]
extern crate tracing;

fn setup_logs() {
    let fmt_layer = fmt::layer().with_target(true);
    let filter_layer = EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new(format!(
        "warn,{}=warn",
        env!("CARGO_CRATE_NAME")
    )));

    if let Ok(journal_layer) = tracing_journald::layer() {
        tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer)
            .with(journal_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer)
            .init();
    }
}

enum CliMode {
    RunApplet,
    Version,
    Help,
    Remote(RemoteCommand),
}

impl CliMode {
    fn flag(&self) -> Option<&'static str> {
        match self {
            Self::RunApplet => None,
            Self::Version => Some("--version"),
            Self::Help => Some("--help"),
            Self::Remote(command) => Some(command.flag()),
        }
    }
}

fn parse_cli(args: impl IntoIterator<Item = String>) -> Result<CliMode, String> {
    let mut mode = CliMode::RunApplet;

    for arg in args {
        let next = match arg.as_str() {
            "-V" | "--version" => CliMode::Version,
            "-h" | "--help" => CliMode::Help,
            _ => match RemoteCommand::from_flag(&arg) {
                Some(command) => CliMode::Remote(command),
                None => return Err(format!("unknown argument: {arg}")),
            },
        };

        if let Some(previous) = mode.flag() {
            return Err(format!(
                "only one command flag can be used at a time: {previous} and {arg}"
            ));
        }

        mode = next;
    }

    Ok(mode)
}

fn print_usage(program: &str) {
    println!("Usage: {program} [--toggle-launcher|--show-launcher|--ping|--version|--help]");
}

fn main() {
    let program = std::env::args()
        .next()
        .unwrap_or_else(|| env!("CARGO_BIN_NAME").to_string());

    match parse_cli(std::env::args().skip(1)) {
        Ok(CliMode::RunApplet) => {}
        Ok(CliMode::Version) => {
            let version = env!("CARGO_PKG_VERSION");
            let commit = option_env!("CLIPBOARD_MANAGER_COMMIT").unwrap_or("unknown");

            println!("clipboard-manager {version} (commit {commit})");
            return;
        }
        Ok(CliMode::Help) => {
            print_usage(&program);
            return;
        }
        Ok(CliMode::Remote(command)) => {
            if let Err(err) = remote::invoke(command) {
                eprintln!("{err}");
                std::process::exit(1);
            }
            return;
        }
        Err(err) => {
            eprintln!("{err}");
            print_usage(&program);
            std::process::exit(2);
        }
    }

    localize::localize();

    setup_logs();

    let (config_handler, config) = match cosmic_config::Config::new(app::APPID, CONFIG_VERSION) {
        Ok(config_handler) => {
            let config = match Config::get_entry(&config_handler) {
                Ok(ok) => ok,
                Err((errs, config)) => {
                    error!("errors loading config: {:?}", errs);
                    config
                }
            };
            (config_handler, config)
        }
        Err(err) => {
            error!("failed to create config handler: {}", err);
            panic!();
        }
    };

    let flags = Flags {
        config_handler,
        config,
    };

    if let Err(e) = cosmic::applet::run::<AppState<db::DbSqlite>>(flags) {
        error!("{e}");
        panic!();
    }
}
