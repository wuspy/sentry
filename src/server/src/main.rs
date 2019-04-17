#[macro_use]
extern crate log;
extern crate simplelog;
extern crate futures;
extern crate tokio;
extern crate gstreamer as gst;

use future::lazy;
use tokio::prelude::*;
use simplelog::*;
use crate::sentry::*;

mod sentry;

fn main() {
    TermLogger::init(LevelFilter::Info, Config::default()).unwrap();
    gst::init().unwrap();

    tokio::run(lazy(|| {
        let mut runtime = tokio::runtime::Runtime::new().unwrap();
        let reactor = runtime.reactor();
        let (ws_sink, ws_source) = sentry::websocket_server::start("127.0.0.1:8080", reactor);
        let (arduino_sink, arduino_source) = sentry::arduino::start("/dev/ttyUSB0", reactor);
        //sentry::video::start("/dev/video3");

        tokio::spawn(arduino_source
            .map_err(|_| ())
            .for_each(move |message| {
                ws_sink.unbounded_send(message);
                Ok(())
            })
        );

        tokio::spawn(ws_source
            .map_err(|_| ())
            .for_each(move |message| {
                arduino_sink.unbounded_send(message);
                Ok(())
            })
        );

        Ok(())
    }));

    info!("Exiting");
}
