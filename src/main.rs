use crate::{
    config::{Mode, OPTIONS},
    types::TrojanError,
};

mod config;
mod proto;
mod proxy;
mod resolver;
mod server;
mod status;
mod sys;
mod tcp_util;
mod tls_conn;
mod types;
mod wintun;

#[tokio::main]
async fn main() {
    config::setup_logger(&OPTIONS.log_file, OPTIONS.log_level);
    if let Err(err) = match OPTIONS.mode {
        Mode::Proxy(_) => {
            log::warn!("trojan started in proxy mode");
            proxy::run()
        }
        Mode::Server(_) => {
            log::warn!("trojan started in server mode");
            server::run()
        }
        #[cfg(target_os = "windows")]
        Mode::Wintun(_) => {
            log::warn!("trojan started in wintun mode");
            wintun::run().await
        }
        #[cfg(not(target_os = "windows"))]
        Mode::Wintun(_) => {
            log::warn!("trojan can't start in wintun mode on a non-windows platform");
            Err(TrojanError::NonWindowsPlatform)
        }
    } {
        log::error!("trojan exited with error:{:?}", err);
    }
}
