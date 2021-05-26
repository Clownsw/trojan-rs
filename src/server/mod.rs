use std::{
    fs::File,
    io::BufReader,
    sync::Arc,
    time::{Duration, Instant},
};

use mio::{net::TcpListener, Events, Interest, Poll, Token};
use rustls::{
    internal::pemfile::{certs, pkcs8_private_keys, rsa_private_keys},
    KeyLogFile, NoClientAuth, ServerConfig,
};

pub use tls_server::TlsServer;

use crate::{config::Opts, resolver::EventedResolver};

mod connection;
mod tcp_backend;
mod tls_server;
mod udp_backend;

const MIN_INDEX: usize = 3;
const MAX_INDEX: usize = usize::MAX / CHANNEL_CNT;
const CHANNEL_CNT: usize = 2;
const CHANNEL_PROXY: usize = 0;
const CHANNEL_BACKEND: usize = 1;
const RESOLVER: usize = 2;
const LISTENER: usize = 1;

fn init_config(opts: &Opts) -> Arc<ServerConfig> {
    let mut config = ServerConfig::new(NoClientAuth::new());
    config.key_log = Arc::new(KeyLogFile::new());
    let cert_file = File::open(opts.server_args().cert.clone()).unwrap();
    let mut buff_reader = BufReader::new(cert_file);
    let cert_chain = certs(&mut buff_reader).unwrap();
    let key_der = {
        let key_file = File::open(opts.server_args().key.clone()).unwrap();
        let mut buff_reader = BufReader::new(key_file);
        let keys = pkcs8_private_keys(&mut buff_reader).unwrap();
        if let Some(key) = keys.get(0) {
            log::info!("pkcs8 private key found");
            key.clone()
        } else {
            let key_file = File::open(opts.server_args().key.clone()).unwrap();
            let mut buff_reader = BufReader::new(key_file);
            let keys = rsa_private_keys(&mut buff_reader).unwrap();
            if let Some(key) = keys.get(0) {
                log::info!("rsa private key found");
                key.clone()
            } else {
                panic!("no private key found");
            }
        }
    };
    config.set_single_cert(cert_chain, key_der).unwrap();
    let mut protocols: Vec<Vec<u8>> = Vec::new();
    for protocol in &opts.server_args().alpn {
        protocols.push(protocol.as_str().into());
    }
    if !protocols.is_empty() {
        config.set_protocols(&protocols);
    }
    Arc::new(config)
}

pub fn run(opts: &mut Opts) {
    let config = init_config(opts);
    let mut poll = Poll::new().unwrap();
    let resolver = EventedResolver::new(&poll, Token(RESOLVER));
    let addr = opts.local_addr.parse().unwrap();
    let mut listener = TcpListener::bind(addr).unwrap();
    poll.registry()
        .register(&mut listener, Token(LISTENER), Interest::READABLE)
        .unwrap();
    let mut server = TlsServer::new(listener, config);
    let mut events = Events::with_capacity(1024);
    let mut last_check_time = Instant::now();
    let check_duration = Duration::new(1, 0);
    loop {
        poll.poll(&mut events, Some(check_duration)).unwrap();
        for event in &events {
            match event.token() {
                Token(LISTENER) => {
                    server.accept(&poll, opts);
                }
                Token(RESOLVER) => {
                    resolver.consume(|(token, ip)| {
                        server.do_conn_resolve(token, &poll, opts, ip, &resolver);
                    });
                }
                _ => {
                    server.do_conn_event(&poll, &event, opts, &resolver);
                }
            }
        }
        let now = Instant::now();
        if now - last_check_time > check_duration {
            server.check_timeout(now, &poll);
            last_check_time = now;
        }
    }
}
