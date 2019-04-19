use hyper::{Body, Method, Response, Server, Request, StatusCode};
use hyper::service::service_fn;
use tokio::prelude::*;
use tokio_fs;
use tokio_io;
use futures::{future, Future};
use std::io;
use std::prelude::*;
use std::net::{SocketAddr, IpAddr};
use std::str::FromStr;
use crate::sentry::config::Config;
use crate::sentry::StartResult;

type ResponseFuture = Box<Future<Item=Response<Body>, Error=io::Error> + Send>;

/// Serves the directory at serve_root on addr
pub fn start(config: Config) -> StartResult<()> {
    let addr = SocketAddr::new("127.0.0.1".parse().unwrap(), config.http_server.port);
    let serve_root = config.http_server.directory.clone();
    info!("Starting HTTP server on {} serving {}", addr, config.http_server.directory);
    tokio::spawn(Server::try_bind(&addr)
        .map_err(|err| format!("Error starting HTTP server: {}", err))?
        .serve(move || {
            let serve_root = config.http_server.directory.clone();
            service_fn(move |req| serve(req, serve_root.clone()))
        })
        .map_err(|_| ())
    );

    Ok(())
}

fn serve(req: Request<Body>, serve_root: String) -> ResponseFuture {
    info!("HTTP {} {}", req.method(), req.uri().path());
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => serve_file(serve_root, "index.html"),
        (&Method::GET, path) => serve_file(serve_root, path),
        _ => {
            Box::new(future::ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap()
            ))
        }
    }
}

fn serve_file(root: String, file: &str) -> ResponseFuture {
    let filename = format!("{}/{}", root, file);
    Box::new(tokio_fs::file::File::open(filename)
        .and_then(|file| {
            let buf: Vec<u8> = Vec::new();
            tokio_io::io::read_to_end(file, buf)
                .and_then(|item| {
                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .body(item.1.into())
                        .unwrap()
                    )
                })
                .or_else(|_| {
                    Ok(Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::empty())
                        .unwrap()
                    )
                })
        })
        .or_else(|_| {
            Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap()
            )
        })
    )
}
