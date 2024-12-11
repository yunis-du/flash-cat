use std::net::{IpAddr, SocketAddr, TcpListener, ToSocketAddrs, UdpSocket};

pub mod net_scout;

/// Find an available port starting from the base port
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

/// Get the IP address for a domain name.
pub fn get_domain_ip(domain: &str) -> Option<IpAddr> {
    let domain = match extract_domain_or_ip(domain) {
        Some(domain) => domain,
        None => domain.to_owned(),
    };
    match (domain, 80).to_socket_addrs() {
        Ok(mut addrs) => match addrs.next() {
            Some(socket_addr) => Some(socket_addr.ip()),
            None => None,
        },
        Err(_) => None,
    }
}

/// Get local IP address.
pub fn get_local_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:53").ok()?;

    let addr = socket.local_addr().ok()?;
    Some(addr.ip())
}

/// Extract the domain or IP address from the given string.
fn extract_domain_or_ip(domain: &str) -> Option<String> {
    let last = domain.split("://").last()?;
    let mut last_by_last = last.split(":");
    let domain = if last_by_last.clone().count() > 1 {
        last_by_last.next()?
    } else {
        last
    };
    if domain.ends_with("/") {
        Some(domain.replace("/", ""))
    } else {
        Some(domain.to_string())
    }
}
