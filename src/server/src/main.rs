#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;
extern crate simplelog;
extern crate futures;
extern crate tokio;
extern crate toml;
extern crate gstreamer as gst;

use futures::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use future::lazy;
use tokio::prelude::*;
use tokio::runtime::Runtime;
use simplelog::{TermLogger, LevelFilter, Config as LogConfig};
use std::prelude::*;
use std::fs;
use crate::sentry::*;
use std::net::{SocketAddr, IpAddr};
use std::str::FromStr;

mod sentry;
use sentry::StartResult;
use sentry::config::Config;

fn main() {
    TermLogger::init(LevelFilter::Info, LogConfig::default()).unwrap();

    let runtime = Runtime::new().unwrap();
    tokio::run(lazy(move || {
        match run(runtime) {
            Ok(()) => Ok(()),
            Err(err) => {
                error!("{}", err);
                panic!("{}", err);
            },
        }
    }));
}

fn run(runtime: Runtime) -> StartResult<()> {
    gst::init().map_err(|err| format!("Could not initialize GStreamer: {}", err))?;

    let reactor = runtime.reactor();
    let config = sentry::config::load()?;
    let (server_tx, server_rx) = sentry::websocket_server::start(config.clone(), reactor)?;
    let (arduino_tx, arduino_rx) = sentry::arduino::start(config.clone(), reactor)?;
    //let (video_tx, video_rx) = sentry::video::start(config.clone())?;
    sentry::http_server::start(config.clone())?;
    sentry::stun_server::start(config.clone())?;

    tokio::spawn(arduino_rx
        .map_err(|_| ())
        .for_each(move |message| {
//            server_tx.unbounded_send(message.clone()).unwrap();
//            video_tx.unbounded_send(message).unwrap();
            Ok(())
        })
    );

    tokio::spawn(server_rx
        .map_err(|_| ())
        .for_each(move |message| {
            arduino_tx.unbounded_send(message.clone()).unwrap();
            //video_tx.unbounded_send(message).unwrap();
            Ok(())
        })
    );

    Ok(())
}