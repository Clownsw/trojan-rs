use std::io::{Read, Write};

use bytes::BytesMut;
use mio::net::TcpStream;
use rustls::{ClientConnection, ServerConnection};

use crate::tls_conn::TlsConn;

macro_rules! impl_tcp_read {
    ($name:ident, $conn:ty) => {
        pub fn $name(
            index: usize,
            mut conn: &TcpStream,
            recv_buf: &mut Vec<u8>,
            server_conn: &mut TlsConn<$conn>,
        ) -> bool {
            loop {
                match conn.read(recv_buf.as_mut_slice()) {
                    Ok(size) => {
                        log::debug!("connection:{} read {} bytes from backend", index, size);
                        if size == 0 {
                            log::warn!("connection:{} meets end of file", index);
                            return false;
                        } else if !server_conn.write_session(&recv_buf.as_slice()[..size]) {
                            return false;
                        }
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        log::debug!("connection:{} read from backend blocked", index);
                        break;
                    }
                    Err(err) => {
                        log::warn!("connection:{} read from backend failed:{}", index, err);
                        return false;
                    }
                }
            }
            true
        }
    };
}

impl_tcp_read!(tcp_read_server, ServerConnection);
impl_tcp_read!(tcp_read_client, ClientConnection);

pub fn tcp_send(
    index: usize,
    mut conn: &TcpStream,
    send_buffer: &mut BytesMut,
    mut data: &[u8],
) -> bool {
    loop {
        if data.is_empty() {
            return true;
        }
        match conn.write(data) {
            Ok(size) => {
                data = &data[size..];
                log::debug!(
                    "connection:{} session write {} byte to backend",
                    index,
                    size
                );
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                log::debug!(
                    "connection:{} session write blocked, remaining:{}",
                    index,
                    data.len()
                );
                send_buffer.extend_from_slice(data);
                break;
            }
            Err(err) => {
                log::warn!("connection:{} send failed:{}", index, err);
                return false;
            }
        }
    }
    true
}
