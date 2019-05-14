#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate futures;
extern crate simplelog;
extern crate tokio;
extern crate toml;
extern crate gstreamer as gst;

use future::lazy;
use tokio::prelude::*;
use tokio::runtime::Runtime;
use simplelog::{TermLogger, LevelFilter, Config as LogConfig, CombinedLogger, WriteLogger, SharedLogger};
use std::env;

mod sentry;
use sentry::StartResult;
use std::fs::File;

/// Broadcasts one receiver to a list of senders
macro_rules! broadcast {
    ($receiver:expr, $senders:expr) => {{
        let senders: Vec<_> = $senders
            .into_iter()
            .map(|sender| sender.clone())
            .collect();

        tokio::spawn($receiver
            .map_err(|_| ())
            .for_each(move |message| {
                senders.iter().for_each(|sender| {
                    sender
                        .unbounded_send(message.clone())
                        .unwrap_or_else(|err| {
                            error!("Failed to send message on bus: {}", err);
                        });
                });
                Ok(())
            })
        );
    }}
}

fn main() {
    let mut log_path = env::current_exe().expect("Cannot get executable path");
    log_path.pop();
    log_path.push("sentry.log");
    let log_path = log_path.to_str().unwrap();
    let mut loggers = Vec::<Box<SharedLogger>>::new();
    if let Ok(log_file) = File::create(log_path) {
        loggers.push(WriteLogger::new(LevelFilter::Info, LogConfig::default(), log_file));
    }
    if let Some(term_logger) = TermLogger::new(LevelFilter::Info, LogConfig::default()) {
        loggers.push(term_logger);
    }
    CombinedLogger::init(loggers).expect("Cannot initialize logging");

    let runtime = Runtime::new().unwrap();
    tokio::run(lazy(move || {
        match run(runtime) {
            Ok(()) => Ok(()),
            Err(err) => {
                error!("{}", err);
                Err(())
            },
        }
    }));
}

fn run(runtime: Runtime) -> StartResult<()> {
    #[allow(deprecated)]
    let reactor = runtime.reactor();
    let config = sentry::config::load()?;
    let (server_tx, server_rx) = sentry::server::start(config.clone())?;
    let (arduino_tx, arduino_rx) = sentry::arduino::start(config.clone(), reactor)?;
    let (video_tx, video_rx) = sentry::video::start(config.clone())?;

    broadcast!(server_rx, vec![&video_tx, &arduino_tx]);
    broadcast!(arduino_rx, vec![&server_tx, &video_tx]);
    broadcast!(video_rx, vec![&server_tx, &arduino_tx]);

    Ok(())
}
