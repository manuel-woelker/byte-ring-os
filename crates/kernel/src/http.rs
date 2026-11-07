//! One boot-owned HTTP connection over a polled, bounded TCP stack.
use super::{clock, files::BootFiles, net};
use byte_ring_drivers::ethernet::Ethernet;
use byte_ring_kernel_core::{
    http::{Request, Response, Status},
    idle::{self, CONNECTION_MS, REQUEST_MS},
};
use smoltcp::{
    iface::{Config, Interface, SocketSet, SocketStorage},
    socket::tcp::{Socket, SocketBuffer, State},
    time::{Duration, Instant},
    wire::{EthernetAddress, IpCidr, Ipv4Address},
};

const PORT: u16 = 8080;

pub fn run(files: &mut BootFiles) -> ! {
    let timer = clock::initialize();
    let (nic, irq) = net::initialize().unwrap_or_else(|error| panic!("HTTP network: {error}"));
    let mut sleeper = super::idle::Idle::new(irq);
    let mut config = Config::new(EthernetAddress(nic.mac).into());
    // Development-only seed; this is not a cryptographic entropy source.
    config.random_seed = clock::counter();
    let mut device = Ethernet::new(nic);
    let mut interface = Interface::new(config, &mut device, Instant::ZERO);
    interface.update_ip_addrs(|addresses| {
        addresses
            .push(IpCidr::new(Ipv4Address::from(net::IP).into(), 24))
            .expect("one IPv4 address fits");
    });
    let mut rx = [0; 2048];
    let mut tx = [0; 1024];
    let mut socket = Socket::new(
        SocketBuffer::new(&mut rx[..]),
        SocketBuffer::new(&mut tx[..]),
    );
    socket.set_timeout(Some(Duration::from_secs(10)));
    socket.set_ack_delay(None);
    socket.set_nagle_enabled(false);
    socket.listen(PORT).expect("valid HTTP listener");
    let mut storage = [SocketStorage::EMPTY];
    let mut sockets = SocketSet::new(&mut storage[..]);
    let handle = sockets.add(socket);
    let mut request = Request::new();
    let mut response: Option<Response> = None;
    let mut position = 0;
    let mut started: Option<i64> = None;
    let mut log_budget = 64;
    net::log(
        files,
        format_args!("HTTP ready http://192.0.2.2:8080/ request_limit=1024 connections=1\r\n"),
    );
    loop {
        let now = timer.milliseconds(clock::counter());
        device.begin_poll();
        interface.poll(Instant::from_millis(now), &mut device, &mut sockets);
        if let Some(error) = device.take_error() {
            panic!("HTTP virtio: {error:?}");
        }
        let mut progress = false;
        let socket = sockets.get_mut::<Socket>(handle);
        if !socket.is_open() {
            // smoltcp permits listen from CLOSED or TIME-WAIT. A fresh tuple
            // and sequence number separate each new connection from old data.
            request = Request::new();
            response = None;
            position = 0;
            started = None;
            socket.listen(PORT).expect("closed HTTP socket can listen");
            progress = true;
        }
        if socket.is_active() {
            let start = *started.get_or_insert(now);
            if now - start >= CONNECTION_MS {
                socket.abort();
                continue;
            }
            if response.is_none() {
                if socket.can_recv() {
                    let mut bytes = [0; 256];
                    match socket.recv_slice(&mut bytes) {
                        Ok(length) => {
                            response = request.push(&bytes[..length]);
                            progress = length > 0;
                        }
                        Err(_) => {
                            socket.abort();
                            continue;
                        }
                    }
                }
                if response.is_none() && socket.state() == State::CloseWait && !socket.can_recv() {
                    response = Some(Response::new(Status::BadRequest, false));
                }
                if response.is_none() && now - start >= REQUEST_MS {
                    if socket.may_send() {
                        response = Some(Response::new(Status::Timeout, false));
                    } else {
                        socket.abort();
                        continue;
                    }
                }
                progress |= response.is_some();
                if let Some(response) = &response
                    && log_budget > 0
                {
                    log_budget -= 1;
                    net::log(
                        files,
                        format_args!("HTTP response {:?}\r\n", response.status),
                    );
                }
            }
            if let Some(response) = &response {
                if position < response.len() && socket.can_send() {
                    match socket.send_slice(response.chunk(position)) {
                        Ok(length) => {
                            position += length;
                            progress |= length > 0;
                        }
                        Err(_) => {
                            socket.abort();
                            continue;
                        }
                    }
                }
                if position == response.len() && socket.may_send() {
                    // TCP retains queued bytes for ACK/retransmission before FIN.
                    socket.close();
                    progress = true;
                }
            }
        }
        if progress {
            continue;
        }
        let now = timer.milliseconds(clock::counter());
        let next = idle::deadline(
            interface
                .poll_at(Instant::from_millis(now), &sockets)
                .map(|time| time.total_millis()),
            started,
            response.is_none(),
        );
        let delay = next.map(|deadline| deadline.saturating_sub(now).max(0) as u64);
        sleeper
            .wait(device.network(), delay)
            .unwrap_or_else(|error| panic!("HTTP idle: {error:?}"));
    }
}
