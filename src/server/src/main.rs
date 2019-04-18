#[macro_use]
extern crate log;
extern crate simplelog;
extern crate futures;
extern crate tokio;
extern crate gstreamer as gst;

use futures::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use future::lazy;
use tokio::prelude::*;
use simplelog::*;
use std::prelude::*;
use crate::sentry::*;
use std::net::SocketAddr;
use std::str::FromStr;

mod sentry;

fn main() {
    TermLogger::init(LevelFilter::Info, Config::default()).unwrap();
    gst::init().unwrap();

    tokio::run(lazy(|| {
        let mut runtime = tokio::runtime::Runtime::new().unwrap();
        let reactor = runtime.reactor();

        let ws_server_address = SocketAddr::from_str("127.0.0.1:8081").unwrap();
        let http_server_address = SocketAddr::from_str("127.0.0.1:8080").unwrap();
        let mut serve_root = std::env::current_exe().unwrap();
        for _ in 0..5 {
            serve_root.pop();
        }
        serve_root.push("dist");

        let video_device = "/dev/video3";
        let arduino_device = "/dev/ttyUSB0";

        let (server_tx, server_rx) = sentry::websocket_server::start(&ws_server_address, reactor);
        let (arduino_tx, arduino_rx) = sentry::arduino::start(arduino_device, reactor);
        //let (video_tx, video_rx) = sentry::video::start(video_device);

        sentry::http_server::start(&http_server_address, serve_root);

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

    info!("Exiting");
}
