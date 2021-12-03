use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc,
    },
    time::{Duration, Instant},
};

use mio::{Poll, Token, Waker};

pub struct DnsEntry {
    pub address: IpAddr,
    pub expired_time: Instant,
}

pub struct DnsResolver {
    waker: Arc<Waker>,
    receiver: Option<Receiver<(Token, String, Option<IpAddr>)>>,
    sender: Sender<(Token, String, Option<IpAddr>)>,
    dns_cache: HashMap<String, DnsEntry>,
    dns_cache_duration: Duration,
    token: Token,
}

impl DnsResolver {
    pub fn new(poll: &Poll, token: Token) -> Self {
        let (sender, receiver) = channel();
        let waker = Arc::new(Waker::new(poll.registry(), token).unwrap());
        Self {
            sender,
            waker,
            token,
            receiver: Some(receiver),
            dns_cache: HashMap::new(),
            dns_cache_duration: Duration::new(10, 0),
        }
    }

    pub fn get_waker(&self) -> Arc<Waker> {
        self.waker.clone()
    }

    pub fn set_cache_timeout(&mut self, timeout: u64) {
        self.dns_cache_duration = Duration::new(timeout, 0);
    }

    pub fn update_dns(&mut self, domain: String, address: IpAddr) {
        log::trace!("update dns cache, {} = {}", domain, address);
        let expired_time = Instant::now() + self.dns_cache_duration;
        self.dns_cache.insert(
            domain,
            DnsEntry {
                address,
                expired_time,
            },
        );
    }

    pub fn query_dns(&mut self, domain: &str) -> Option<IpAddr> {
        if let Some(entry) = self.dns_cache.get(domain) {
            log::debug!("found {} = {} in dns cache", domain, entry.address);
            if entry.expired_time > Instant::now() {
                return Some(entry.address);
            } else {
                log::info!("domain {} expired, remove from cache", domain);
                let _ = self.dns_cache.remove(domain);
            }
        }
        log::info!("domain {} not found in cache", domain);
        None
    }

    pub fn resolve(&self, domain: String, token: Option<Token>) {
        let token = token.unwrap_or(self.token);
        log::info!("resolve domain:{} with token:{}", domain, token.0);
        let sender = self.sender.clone();
        let waker = self.waker.clone();
        rayon::spawn(move || {
            log::info!("thread resolve domain:{} with token:{}", domain, token.0);
            let mut address = None;
            if let Ok(ips) = dns_lookup::lookup_host(domain.as_str()) {
                for addr in ips {
                    if address.is_none() || addr.is_ipv4() {
                        address.replace(addr);
                    }
                    if address.as_ref().unwrap().is_ipv4() {
                        break;
                    }
                }
            }
            if let Err(err) = sender.send((token, domain.clone(), address)) {
                log::error!("send resolver result failed:{:?}", err);
            } else if let Err(err) = waker.wake() {
                log::error!("wake failed {}", err);
            } else {
                log::info!("domain:{} resolved and wake poll", domain);
            }
        });
    }

    pub fn consume<F: FnMut(Token, Option<IpAddr>)>(&mut self, mut f: F) {
        let receiver = self.receiver.take().unwrap();
        receiver.try_iter().for_each(|(token, domain, ip)| {
            if let Some(ip) = ip {
                self.update_dns(domain, ip);
            }
            f(token, ip);
        });
        self.receiver.replace(receiver);
    }
}
