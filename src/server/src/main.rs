#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate futures;
extern crate gstreamer as gst;
extern crate simplelog;
extern crate tokio;
extern crate toml;

use future::lazy;
use simplelog::{
    CombinedLogger, Config as LogConfig, LevelFilter, SharedLogger, TermLogger, WriteLogger,
};
use std::env;
use tokio::prelude::*;
use tokio::runtime::Runtime;

mod sentry;
use crate::sentry::{bus, Message};
use futures::future::Loop;
use std::fs::File;
use std::ops::Add;
use std::time::{Duration, Instant};
use tokio::timer::Delay;

fn main() {
    let mut log_path = env::current_exe().expect("Cannot get executable path");
    log_path.pop();
    log_path.push("sentry.log");
    let log_path = log_path.to_str().unwrap();
    let mut loggers = Vec::<Box<SharedLogger>>::new();
    if let Ok(log_file) = File::create(log_path) {
        loggers.push(WriteLogger::new(
            LevelFilter::Info,
            LogConfig::default(),
            log_file,
        ));
    }
    if let Some(term_logger) = TermLogger::new(LevelFilter::Info, LogConfig::default()) {
        loggers.push(term_logger);
    }
    CombinedLogger::init(loggers).expect("Cannot initialize logging");

    let runtime = Runtime::new().unwrap();
    tokio::run(lazy(move || run(runtime)).map_err(|err| error!("{}", err)));
}

fn run(runtime: Runtime) -> impl Future<Item = (), Error = String> {
    let (bus_sink, bus_stream) = bus::new::<Message>();

    future::result(sentry::config::load()).and_then(move |config| {
        let video = {
            let config = config.clone();
            let bus = (bus_sink.clone(), bus_stream.clone());
            move || sentry::video::start(config, bus)
        };
        let server = {
            let config = config.clone();
            let bus = (bus_sink.clone(), bus_stream.clone());
            move || sentry::server::start(config, bus)
        };
        let arduino = {
            let config = config.clone();
            let bus = (bus_sink.clone(), bus_stream.clone());
            #[allow(deprecated)]
            let reactor = runtime.reactor().clone();
            move || sentry::arduino::start(config, bus, &reactor)
        };

        tokio::spawn(run_module(format!("Video"), video));
        tokio::spawn(run_module(format!("Server"), server));
        tokio::spawn(run_module(format!("Arduino"), arduino));

        bus_stream
            .map_err(|_| format!("Failed to read from bus"))
            .for_each(|_| Ok(()))
    })
}

fn run_module<T, M>(name: String, module: T) -> impl Future<Item = (), Error = ()>
where
    T: FnOnce() -> M + Clone,
    M: Future<Item = (), Error = String>,
{
    future::loop_fn((name, module), move |(name, module)| {
        info!("Starting module {}", name);
        module.clone()()
            .and_then({
                let name = name.clone();
                move |_| {
                    info!("Module {} stopped without error", name);
                    Ok(Loop::Break(()))
                }
            })
            .or_else(move |err| {
                error!("Module {} failed with error: {}", name, err);
                info!("Restarting module {} in 5 seconds...", name);
                Delay::new(Instant::now().add(Duration::from_secs(5)))
                    .map_err(|_| ())
                    .and_then(|_| Ok(Loop::Continue((name, module))))
            })
    })
}
