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
use simplelog::{TermLogger, LevelFilter, Config as LogConfig};
use std::prelude::*;
use std::fs;
use crate::sentry::*;
use std::net::{SocketAddr, IpAddr};
use std::str::FromStr;

mod sentry;
use sentry::config::Config;

fn main() {
    TermLogger::init(LevelFilter::Info, LogConfig::default()).unwrap();
    if let Err(err) = gst::init() {
        error!("Could not initial GStreamer: {}", err);
        return;
    }

    match sentry::config::load() {
        Ok(config) => run(config),
        Err(err) => error!("{}", err)
    }
}

fn run(config: Config) {
    tokio::run(lazy(move || {
        let mut runtime = tokio::runtime::Runtime::new().unwrap();
        let reactor = runtime.reactor();

        let (server_tx, server_rx) = sentry::websocket_server::start(config.clone(), reactor);
        let (arduino_tx, arduino_rx) = sentry::arduino::start(config.clone(), reactor);
        //let (video_tx, video_rx) = sentry::video::start(&config);

        sentry::http_server::start(config.clone());

        tokio::spawn(arduino_rx
            .map_err(|_| ())
            .for_each(move |message| {
                // TODO
                Ok(())
            })
        );

        tokio::spawn(server_rx
            .map_err(|_| ())
            .for_each(move |message| {
                match message.content {
                    MessageContent::Command(_) => {
                        arduino_tx.unbounded_send(message).unwrap();
                    },
                    MessageContent::WebRtcOffer { .. } => {
                        //video_tx.unbounded_send(message).unwrap();
                    }
                    _ => {},
                }
                Ok(())
            })
        );

        Ok(())
    }));
}