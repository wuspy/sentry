use crate::sentry::config::Config;
use crate::sentry::StartResult;
use std::process::Command;

pub fn start(config: Config) -> StartResult<()> {
    info!("Starting STUN server on {}:{}", &config.video.stun_host, config.video.stun_port);
    Command::new("stunserver")
        .args(&[
            "--primaryinterface", config.video.stun_host.as_str(),
            "--primaryport", format!("{}", config.video.stun_port).as_str(),
        ])
        .spawn()
        .map_err(|err| format!("Failed to start STUN server: {}", err))?;

    Ok(())
}
