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

#[derive(Deserialize)]
struct ArduinoConfig {
    device: String,
}

#[derive(Deserialize)]
struct VideoConfig {
    device: String,
}

#[derive(Deserialize)]
struct WsServerConfig {
    port: u16,
}

#[derive(Deserialize)]
struct HttpServerConfig {
    port: u16,
    directory: String,
}

#[derive(Deserialize)]
struct Config {
    websocket: WsServerConfig,
    http_server: HttpServerConfig,
    video: VideoConfig,
    arduino: ArduinoConfig,
}

fn main() {
    TermLogger::init(LevelFilter::Info, LogConfig::default()).unwrap();
    if let Err(err) = gst::init() {
        error!("Could not initial GStreamer: {}", err);
        return;
    }

    match read_config() {
        Ok(config) => run(config),
        Err(err) => error!("{}", err)
    }
}

fn read_config() -> Result<Config, String> {
    let mut path = std::env::current_exe().map_err(|err| err.to_string())?;
    path.pop();
    path.push("config.toml");
    let path = path.to_str().unwrap();
    info!("Reading configuration at \"{}\"...", path);

    Ok(toml::from_str::<Config>(
        fs::read_to_string(path)
            .map_err(|err|
                format!("Could not read configuration file \"{}\": {}", path, err.to_string())
            )?
            .as_str()
        )
        .map_err(|err|
            format!("Could not parse configuration file \"{}\": {}", path, err.to_string())
        )?
    )
}

fn run(config: Config) {
    tokio::run(lazy(|| {
        let mut runtime = tokio::runtime::Runtime::new().unwrap();
        let reactor = runtime.reactor();

        let ip = IpAddr::from_str("127.0.0.1").unwrap();

        let (server_tx, server_rx) = sentry::websocket_server::start(&SocketAddr::new(ip, config.websocket.port), reactor);
        let (arduino_tx, arduino_rx) = sentry::arduino::start(config.arduino.device, reactor);
        //let (video_tx, video_rx) = sentry::video::start(config.video.device);

        sentry::http_server::start(&SocketAddr::new(ip, config.http_server.port), config.http_server.directory);

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