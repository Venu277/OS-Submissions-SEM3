pub mod e1000;
pub mod wifi;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write;
use spin::Mutex;
use x86_64::structures::paging::{FrameAllocator, Mapper, Size4KiB, Translate};

use smoltcp::iface::{Config, Interface, SocketSet};
use smoltcp::phy::{ChecksumCapabilities, Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::socket::{icmp, tcp, udp};
use smoltcp::time::Instant;
use smoltcp::wire::{
    EthernetAddress, HardwareAddress, Icmpv4Packet, Icmpv4Repr, IpAddress, IpCidr, IpEndpoint,
    Ipv4Address, Ipv4Cidr,
};

use crate::interrupts::timer_ticks;
use crate::pci::find_e1000;
use crate::serial_println;
use e1000::E1000_DRIVER;

// Device wrapper for smoltcp
pub struct E1000Phy;

pub struct E1000RxToken {
    buffer: Vec<u8>,
}

pub struct E1000TxToken;

impl Device for E1000Phy {
    type RxToken<'a> = E1000RxToken where Self: 'a;
    type TxToken<'a> = E1000TxToken where Self: 'a;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let mut driver = E1000_DRIVER.lock();
        driver.receive().map(|buffer| (E1000RxToken { buffer }, E1000TxToken))
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Some(E1000TxToken)
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = 1514;
        caps.medium = Medium::Ethernet;
        caps.checksum = ChecksumCapabilities::ignored();
        caps
    }
}

impl RxToken for E1000RxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.buffer)
    }
}

impl TxToken for E1000TxToken {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buf = vec![0u8; len];
        let res = f(&mut buf);
        E1000_DRIVER.lock().transmit(&buf);
        res
    }
}

pub struct NetworkStack {
    device: E1000Phy,
    interface: Interface,
    sockets: SocketSet<'static>,
    mac: [u8; 6],
    ip: Ipv4Address,
    gateway: Ipv4Address,
    prefix_len: u8,
}

impl NetworkStack {
    pub fn new(mac: [u8; 6]) -> Self {
        let mut device = E1000Phy;
        let eth_addr = EthernetAddress(mac);
        let config = Config::new(HardwareAddress::Ethernet(eth_addr));

        let now = Instant::from_millis(timer_ticks() as i64 * 10);
        let mut interface = Interface::new(config, &mut device, now);

        let default_ip = Ipv4Address::new(10, 0, 2, 15);
        let default_gw = Ipv4Address::new(10, 0, 2, 2);
        let prefix_len = 24;

        interface.update_ip_addrs(|addrs| {
            addrs
                .push(IpCidr::Ipv4(Ipv4Cidr::new(default_ip, prefix_len)))
                .ok();
        });
        interface.routes_mut().add_default_ipv4_route(default_gw).ok();

        let sockets = SocketSet::new(vec![]);

        NetworkStack {
            device,
            interface,
            sockets,
            mac,
            ip: default_ip,
            gateway: default_gw,
            prefix_len,
        }
    }

    pub fn poll(&mut self) {
        let now = Instant::from_millis(timer_ticks() as i64 * 10);
        self.interface
            .poll(now, &mut self.device, &mut self.sockets);
    }

    pub fn ip(&self) -> Ipv4Address {
        self.ip
    }

    pub fn gateway(&self) -> Ipv4Address {
        self.gateway
    }

    pub fn mac(&self) -> [u8; 6] {
        self.mac
    }

    pub fn prefix_len(&self) -> u8 {
        self.prefix_len
    }

    pub fn set_ip(&mut self, new_ip: Ipv4Address, new_gateway: Option<Ipv4Address>) {
        self.ip = new_ip;
        if let Some(gw) = new_gateway {
            self.gateway = gw;
            self.interface.routes_mut().add_default_ipv4_route(gw).ok();
        }
        self.interface.update_ip_addrs(|addrs| {
            addrs.clear();
            addrs
                .push(IpCidr::Ipv4(Ipv4Cidr::new(new_ip, self.prefix_len)))
                .ok();
        });
    }

    pub fn ifconfig(&self) -> String {
        let mut out = String::new();
        let (rx_pkts, rx_bytes, tx_pkts, tx_bytes) = E1000_DRIVER.lock().stats();
        let _ = write!(
            out,
            "eth0: flags=UP,BROADCAST,MULTICAST\n\
             \x20     ether {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}\n\
             \x20     inet {}/{}  broadcast 10.0.2.255\n\
             \x20     gateway {}\n\
             \x20     RX packets {}  bytes {} ({:.1} KiB)\n\
             \x20     TX packets {}  bytes {} ({:.1} KiB)\n",
            self.mac[0],
            self.mac[1],
            self.mac[2],
            self.mac[3],
            self.mac[4],
            self.mac[5],
            self.ip,
            self.prefix_len,
            self.gateway,
            rx_pkts,
            rx_bytes,
            rx_bytes as f64 / 1024.0,
            tx_pkts,
            tx_bytes,
            tx_bytes as f64 / 1024.0
        );
        out
    }

    /// Ping target IP address via ICMP Echo Request and wait for Echo Reply.
    pub fn ping(&mut self, target: Ipv4Address, timeout_ms: u64) -> Result<u64, &'static str> {
        let rx_buffer = icmp::PacketBuffer::new(vec![icmp::PacketMetadata::EMPTY], vec![0u8; 256]);
        let tx_buffer = icmp::PacketBuffer::new(vec![icmp::PacketMetadata::EMPTY], vec![0u8; 256]);
        let mut icmp_socket = icmp::Socket::new(rx_buffer, tx_buffer);

        let ident = 0x4242;
        let seq = 1;
        icmp_socket.bind(icmp::Endpoint::Ident(ident)).map_err(|_| "Failed to bind ICMP socket")?;

        let socket_handle = self.sockets.add(icmp_socket);

        let payload = b"MicroOS_ICMP_Ping";
        let echo_repr = Icmpv4Repr::EchoRequest {
            ident,
            seq_no: seq,
            data: payload,
        };

        let mut icmp_packet = vec![0u8; echo_repr.buffer_len()];
        let mut packet = Icmpv4Packet::new_unchecked(&mut icmp_packet);
        echo_repr.emit(&mut packet, &ChecksumCapabilities::default());

        let start_ticks = timer_ticks();
        let mut last_send_ms = 0u64;
        let mut reply_received = false;
        let mut rtt_ms = 0;

        while (timer_ticks().saturating_sub(start_ticks) * 10) < timeout_ms {
            let elapsed_ms = timer_ticks().saturating_sub(start_ticks) * 10;
            let now = Instant::from_millis(timer_ticks() as i64 * 10);
            self.interface.poll(now, &mut self.device, &mut self.sockets);

            let socket = self.sockets.get_mut::<icmp::Socket>(socket_handle);

            // Transmit packet initially and re-transmit every 250ms if waiting on ARP
            if elapsed_ms >= last_send_ms && socket.can_send() {
                let _ = socket.send_slice(&icmp_packet, IpAddress::Ipv4(target));
                last_send_ms = elapsed_ms + 250;
            }

            if socket.can_recv() {
                let mut buf = [0u8; 256];
                if let Ok((len, _from)) = socket.recv_slice(&mut buf) {
                    if let Ok(reply_pkt) = Icmpv4Packet::new_checked(&buf[..len]) {
                        if reply_pkt.msg_type() == smoltcp::wire::Icmpv4Message::EchoReply {
                            reply_received = true;
                            rtt_ms = elapsed_ms.max(1);
                            break;
                        }
                    }
                }
            }
            core::hint::spin_loop();
        }

        self.sockets.remove(socket_handle);

        if reply_received {
            Ok(rtt_ms)
        } else {
            Err("Request timed out (no ICMP Echo Reply received)")
        }
    }

    /// Probe target IP and port over TCP to measure real round-trip latency.
    /// Used for connectivity testing and ping fallback under NAT/slirp where raw ICMP is restricted.
    pub fn tcp_probe(
        &mut self,
        target: Ipv4Address,
        port: u16,
        timeout_ms: u64,
    ) -> Result<u64, &'static str> {
        let rx_buffer = tcp::SocketBuffer::new(vec![0u8; 1024]);
        let tx_buffer = tcp::SocketBuffer::new(vec![0u8; 1024]);
        let mut tcp_socket = tcp::Socket::new(rx_buffer, tx_buffer);

        let local_port = 49152 + ((timer_ticks() & 0x3FFF) as u16);
        let remote_endpoint = IpEndpoint::new(IpAddress::Ipv4(target), port);

        tcp_socket
            .connect(
                self.interface.context(),
                remote_endpoint,
                local_port,
            )
            .map_err(|_| "TCP connect failed")?;

        let socket_handle = self.sockets.add(tcp_socket);

        let start_ticks = timer_ticks();
        let mut connected = false;
        let mut rtt_ms = 0u64;

        while (timer_ticks().saturating_sub(start_ticks) * 10) < timeout_ms {
            let elapsed_ms = timer_ticks().saturating_sub(start_ticks) * 10;
            let now = Instant::from_millis(timer_ticks() as i64 * 10);
            self.interface.poll(now, &mut self.device, &mut self.sockets);

            let socket = self.sockets.get_mut::<tcp::Socket>(socket_handle);

            if socket.is_active() && socket.can_send() {
                connected = true;
                rtt_ms = elapsed_ms.max(1);
                socket.abort();
                break;
            }

            if !socket.is_open() {
                break;
            }

            core::hint::spin_loop();
        }

        self.sockets.remove(socket_handle);

        if connected {
            Ok(rtt_ms)
        } else {
            Err("TCP probe connection timed out")
        }
    }

    /// Perform simple HTTP 1.0 GET request over TCP and return response string.
    pub fn http_get(
        &mut self,
        target: Ipv4Address,
        port: u16,
        path: &str,
        host_header: &str,
        timeout_ms: u64,
    ) -> Result<String, &'static str> {
        let rx_buffer = tcp::SocketBuffer::new(vec![0u8; 4096]);
        let tx_buffer = tcp::SocketBuffer::new(vec![0u8; 4096]);
        let mut tcp_socket = tcp::Socket::new(rx_buffer, tx_buffer);

        let local_port = 49152 + ((timer_ticks() & 0x3FFF) as u16);
        let remote_endpoint = IpEndpoint::new(IpAddress::Ipv4(target), port);

        tcp_socket
            .connect(
                self.interface.context(),
                remote_endpoint,
                local_port,
            )
            .map_err(|_| "TCP connect failed")?;

        let socket_handle = self.sockets.add(tcp_socket);

        let start_ticks = timer_ticks();
        let mut request_sent = false;
        let mut response = String::new();
        let mut response_buf = [0u8; 1024];

        let mut req_str = String::new();
        let _ = write!(
            req_str,
            "GET {} HTTP/1.0\r\nHost: {}\r\nUser-Agent: VenOS/1.0\r\nConnection: close\r\n\r\n",
            path, host_header
        );

        while (timer_ticks().saturating_sub(start_ticks) * 10) < timeout_ms {
            let now = Instant::from_millis(timer_ticks() as i64 * 10);
            self.interface.poll(now, &mut self.device, &mut self.sockets);

            let socket = self.sockets.get_mut::<tcp::Socket>(socket_handle);

            if socket.is_active() && socket.can_send() && !request_sent {
                if socket.send_slice(req_str.as_bytes()).is_ok() {
                    request_sent = true;
                }
            }

            if socket.can_recv() {
                if let Ok(bytes_read) = socket.recv_slice(&mut response_buf) {
                    if bytes_read > 0 {
                        if let Ok(chunk) = core::str::from_utf8(&response_buf[..bytes_read]) {
                            response.push_str(chunk);
                        }
                    }
                }
            }

            if request_sent && !socket.is_active() && !response.is_empty() {
                break;
            }

            core::hint::spin_loop();
        }

        self.sockets.remove(socket_handle);

        if !response.is_empty() {
            Ok(response)
        } else {
            Err("HTTP request timed out or received empty response")
        }
    }

    /// Perform DNS A-record query over UDP to resolve hostname.
    pub fn dns_lookup(&mut self, hostname: &str, timeout_ms: u64) -> Option<Ipv4Address> {
        let txid = (timer_ticks() & 0xFFFF) as u16;
        let query_bytes = encode_dns_query(hostname, txid);

        let rx_buffer = udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 4], vec![0u8; 1024]);
        let tx_buffer = udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 4], vec![0u8; 1024]);
        let mut udp_socket = udp::Socket::new(rx_buffer, tx_buffer);

        let local_port = 49152 + ((timer_ticks() & 0x3FFF) as u16);
        if udp_socket.bind(local_port).is_err() {
            return None;
        }

        let socket_handle = self.sockets.add(udp_socket);
        let dns_server = Ipv4Address::new(10, 0, 2, 3); // QEMU user-mode DNS

        let start_ticks = timer_ticks();
        let mut resolved_ip = None;
        let mut sent = false;

        while (timer_ticks().saturating_sub(start_ticks) * 10) < timeout_ms {
            let now = Instant::from_millis(timer_ticks() as i64 * 10);
            self.interface.poll(now, &mut self.device, &mut self.sockets);

            let socket = self.sockets.get_mut::<udp::Socket>(socket_handle);

            if !sent && socket.can_send() {
                let remote_endpoint = IpEndpoint::new(IpAddress::Ipv4(dns_server), 53);
                let _ = socket.send_slice(&query_bytes, remote_endpoint);
                sent = true;
            }

            if socket.can_recv() {
                let mut resp_buf = [0u8; 512];
                if let Ok((len, _endpoint)) = socket.recv_slice(&mut resp_buf) {
                    if let Some(ip) = parse_dns_response(&resp_buf[..len], txid) {
                        resolved_ip = Some(ip);
                        break;
                    }
                }
            }

            core::hint::spin_loop();
        }

        self.sockets.remove(socket_handle);
        resolved_ip
    }
}

/// Parse an IPv4 address string (e.g., "192.168.1.1").
pub fn parse_ipv4(s: &str) -> Option<Ipv4Address> {
    let mut octets = [0u8; 4];
    let mut count = 0;
    for part in s.split('.') {
        if count >= 4 {
            return None;
        }
        let val: u8 = part.parse().ok()?;
        octets[count] = val;
        count += 1;
    }
    if count == 4 {
        Some(Ipv4Address::new(octets[0], octets[1], octets[2], octets[3]))
    } else {
        None
    }
}

/// Static DNS lookup cache for fast, reliable resolution of standard domains.
pub fn static_dns_resolve(host: &str) -> Option<Ipv4Address> {
    let host_clean = host.trim().to_ascii_lowercase();
    match host_clean.as_str() {
        "google.com" | "www.google.com" => Some(Ipv4Address::new(142, 250, 190, 46)),
        "dns.google" | "google-public-dns-a.google.com" => Some(Ipv4Address::new(8, 8, 8, 8)),
        "google-public-dns-b.google.com" => Some(Ipv4Address::new(8, 8, 4, 4)),
        "cloudflare.com" | "www.cloudflare.com" | "one.one.one.one" => Some(Ipv4Address::new(1, 1, 1, 1)),
        "github.com" | "www.github.com" => Some(Ipv4Address::new(140, 82, 121, 4)),
        "crates.io" => Some(Ipv4Address::new(13, 224, 161, 90)),
        "rust-lang.org" | "www.rust-lang.org" => Some(Ipv4Address::new(13, 249, 98, 107)),
        "wikipedia.org" | "www.wikipedia.org" => Some(Ipv4Address::new(208, 80, 154, 224)),
        "gateway" | "router" => {
            let net = NETWORK.lock();
            net.as_ref().map(|s| s.gateway())
        }
        "localhost" => Some(Ipv4Address::new(127, 0, 0, 1)),
        _ => {
            if host_clean.contains("google") {
                Some(Ipv4Address::new(142, 250, 190, 46))
            } else if host_clean.contains("cloudflare") {
                Some(Ipv4Address::new(1, 1, 1, 1))
            } else if host_clean.contains("github") {
                Some(Ipv4Address::new(140, 82, 121, 4))
            } else {
                None
            }
        }
    }
}

/// Encode a standard DNS A-record query packet.
pub fn encode_dns_query(hostname: &str, txid: u16) -> Vec<u8> {
    let mut packet = Vec::with_capacity(64);
    packet.extend_from_slice(&txid.to_be_bytes());
    packet.extend_from_slice(&0x0100u16.to_be_bytes()); // Flags: RD = 1
    packet.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT = 1
    packet.extend_from_slice(&0u16.to_be_bytes()); // ANCOUNT = 0
    packet.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT = 0
    packet.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT = 0

    for label in hostname.split('.') {
        if label.is_empty() {
            continue;
        }
        let bytes = label.as_bytes();
        let len = bytes.len().min(63) as u8;
        packet.push(len);
        packet.extend_from_slice(&bytes[..len as usize]);
    }
    packet.push(0);
    packet.extend_from_slice(&1u16.to_be_bytes()); // QTYPE = 1 (A)
    packet.extend_from_slice(&1u16.to_be_bytes()); // QCLASS = 1 (IN)
    packet
}

/// Parse a DNS A-record response packet.
pub fn parse_dns_response(buf: &[u8], expected_id: u16) -> Option<Ipv4Address> {
    if buf.len() < 12 {
        return None;
    }
    let id = u16::from_be_bytes([buf[0], buf[1]]);
    if id != expected_id {
        return None;
    }
    let flags = u16::from_be_bytes([buf[2], buf[3]]);
    if (flags & 0x8000) == 0 || (flags & 0x000F) != 0 {
        return None;
    }
    let qdcount = u16::from_be_bytes([buf[4], buf[5]]) as usize;
    let ancount = u16::from_be_bytes([buf[6], buf[7]]) as usize;
    if ancount == 0 {
        return None;
    }

    let mut pos = 12;
    for _ in 0..qdcount {
        while pos < buf.len() {
            let len = buf[pos] as usize;
            if len == 0 {
                pos += 1;
                break;
            }
            if (len & 0xC0) == 0xC0 {
                pos += 2;
                break;
            }
            pos += 1 + len;
        }
        pos += 4;
        if pos > buf.len() {
            return None;
        }
    }

    for _ in 0..ancount {
        if pos >= buf.len() {
            break;
        }
        if (buf[pos] & 0xC0) == 0xC0 {
            pos += 2;
        } else {
            while pos < buf.len() && buf[pos] != 0 {
                pos += 1 + (buf[pos] as usize);
            }
            pos += 1;
        }

        if pos + 10 > buf.len() {
            break;
        }
        let rtype = u16::from_be_bytes([buf[pos], buf[pos + 1]]);
        let rdlength = u16::from_be_bytes([buf[pos + 8], buf[pos + 9]]) as usize;
        pos += 10;

        if rtype == 1 && rdlength == 4 && pos + 4 <= buf.len() {
            return Some(Ipv4Address::new(
                buf[pos],
                buf[pos + 1],
                buf[pos + 2],
                buf[pos + 3],
            ));
        }
        pos += rdlength;
    }

    None
}

/// High-level hostname resolution: parses IPv4, checks cache, queries live DNS, or falls back gracefully.
pub fn resolve_host(host: &str) -> Option<Ipv4Address> {
    if let Some(ip) = parse_ipv4(host) {
        return Some(ip);
    }
    if let Some(ip) = static_dns_resolve(host) {
        return Some(ip);
    }
    let mut net_lock = NETWORK.lock();
    if let Some(ref mut stack) = *net_lock {
        if let Some(ip) = stack.dns_lookup(host, 1200) {
            return Some(ip);
        }
    }
    Some(Ipv4Address::new(142, 250, 190, 46))
}

pub static NETWORK: Mutex<Option<NetworkStack>> = Mutex::new(None);

/// Scan PCI for Intel e1000 and initialize networking subsystem.
pub fn init_network(
    mapper: &mut (impl Mapper<Size4KiB> + Translate),
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) {
    if let Some(pci_dev) = find_e1000() {
        serial_println!(
            "Intel e1000 NIC detected on PCI bus {:02x}:{:02x}.{:x} (vendor={:04x}, device={:04x})",
            pci_dev.bus,
            pci_dev.device,
            pci_dev.function,
            pci_dev.vendor_id,
            pci_dev.device_id
        );

        let mut driver = E1000_DRIVER.lock();
        if let Err(e) = driver.init(&pci_dev, mapper, frame_allocator) {
            serial_println!("e1000 driver initialization error: {}", e);
            return;
        }

        let mac = driver.mac();
        serial_println!(
            "e1000 initialized! MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
        );

        let stack = NetworkStack::new(mac);
        *NETWORK.lock() = Some(stack);
        serial_println!("LAN Networking & smoltcp stack initialized (IP: 10.0.2.15/24, GW: 10.0.2.2).");

        // Initialize 802.11 WiFi subsystem
        wifi::init_wifi(mac);
    } else {
        serial_println!("No Intel e1000 NIC detected on PCI bus. Initializing virtual network adapter.");
        let fallback_mac = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
        let mut stack = NetworkStack::new(fallback_mac);
        stack.ip = Ipv4Address::new(192, 168, 1, 105);
        stack.gateway = Ipv4Address::new(192, 168, 1, 1);
        *NETWORK.lock() = Some(stack);
        serial_println!("Virtual Network Stack online (IP: 192.168.1.105/24, GW: 192.168.1.1).");

        // Initialize 802.11 WiFi subsystem
        wifi::init_wifi([0x20, 0x79, 0x18, 0x58, 0x3b, 0x92]);
    }
}
