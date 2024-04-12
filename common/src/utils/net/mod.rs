use std::net::{SocketAddr, TcpListener};

pub mod net_scout;

pub fn find_available_port(base_port: u16) -> u16 {
    let mut port = base_port;
    loop {
        let addr: SocketAddr = format!("0.0.0.0:{}", port).parse().unwrap();
        if !TcpListener::bind(&addr).is_err() {
            break port;
        }
        port += 1;
    }
}
