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

use serde_json::json;

mod sentry;
use sentry::StartResult;
use sentry::config::Config;

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
                    sender.unbounded_send(message.clone());
                });
                Ok(())
            })
        );
    }}
}

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
    let reactor = runtime.reactor();
    let config = sentry::config::load()?;
    let (server_tx, server_rx) = sentry::server::start(config.clone())?;
    //let (arduino_tx, arduino_rx) = sentry::arduino::start(config.clone(), reactor)?;
    let (video_tx, video_rx) = sentry::video::start(config.clone())?;

    broadcast!(server_rx, vec![&video_tx]);
    //broadcast!(arduino_rx, vec![&server_tx, &video_tx]);
    broadcast!(video_rx, vec![&server_tx]);

    Ok(())
}
