#![allow(dead_code)]

use std::time::{Duration, Instant};

use etherparse::{IpTrafficClass, Ipv4Header, TcpHeader};

struct EchoServer;
impl Service for EchoServer {
    fn on_connect(&mut self, data: &[u8]) -> Response {
        println!("Connected: {:?}", data);
        return Response::Data("Welcome to echo server!".as_bytes().into());
    }

    fn on_receive(&mut self, data: &[u8]) -> Response {
        println!("Received: {:?}", data);
        let mut out = Vec::new();
        out.extend_from_slice("Echo :".as_bytes());
        out.extend_from_slice(data);
        return Response::Data(out);
    }

    fn on_reset(&mut self) {
        println!("Connection Reset");
    }

    fn on_close(&mut self, data: &[u8]) {
        println!("Closed: {:?}", data);
    }
}

enum Response {
    None,
    Data(Vec<u8>),
    Close(Vec<u8>)
}

struct HTTPServer;

impl Service for HTTPServer {
    fn on_connect(&mut self, data: &[u8]) -> Response {
        println!("Connected: {:?}", data);
        Response::None
    }

    fn on_receive(&mut self, data: &[u8]) -> Response {
        println!("Received: {:?}", data);
        let s = String::from_utf8(data.into()).unwrap();
        let line1 = s.lines().next().unwrap();
        let filename = line1.split_whitespace().nth(1).unwrap();
        println!("GET request for: {}", filename);
        if filename == "/" {
            let response = r#"
HTTP/1.0 200 OK

<html>
<body>
<h1> Welcome to Simple Webserver </h1>
<br>
<h2>Links:</h2>
<a href='/hello_world'>/hello_world</a>
<br>
</body>
</html>
            "#;
            return Response::Close(response.as_bytes().into());
        }

        if filename == "/hello_world" {
            let response = r#"
HTTP/1.0 200 OK

<html>
<body>
<h1> Hello World </h1>
TCP is awesome.
</body>
</html>
            "#;
            return Response::Close(response.as_bytes().into());
        }
        let response = r#"
HTTP/1.0 404 NOT FOUND

<html>
<body>
<h1> Requested file is not found.</h1>
</body>
</html>
            "#;
        return Response::Close(response.as_bytes().into());
    }

    fn on_reset(&mut self) {
        println!("Connection Reset");
    }

    fn on_close(&mut self, data: &[u8]) {
        println!("Closed: {:?}", data);
    }
}

fn main() {
    let mut iface = tun_tap::Iface::without_packet_info("", tun_tap::Mode::Tun)
        .expect("Failed to initialize TUN interface");
    loop {
        let mut tcp = {
            TCP::with_iface(
                iface,
                Box::new(HTTPServer),
                ([10, 0, 0, 2], 1000),
                None,
                OpenMode::Passive,
            )
        };

        while tcp.tcb.state != TCPState::Closed {
            tcp.tick();
        }

        iface = tcp.iface;
    }
}

struct TCP {
    iface: tun_tap::Iface,
    buf: [u8; 2000],
    window_size: u16,
    tcb: TCB,
}

type Segment = (Ipv4Header, TcpHeader, Vec<u8>);

#[derive(Eq, PartialEq, Debug)]
enum TCPState {
    Closed,
    Listen,
    SynSent,
    SynRecvd,
    Estab,
    FinWait1,
    FinWait2,
    CloseWait,
    Closing,
    LastAck,
    TimeWait,
}

#[derive(Eq, PartialEq)]
enum OpenMode {
    Active,
    Passive,
}

struct TCB {
    state: TCPState,
    svc: Box<dyn Service>,
    local_socket: ([u8; 4], u16),
    foreign_socket: Option<([u8; 4], u16)>,
    open_mode: OpenMode,
    irs: u32,
    iss: u32,
    snd_una: u32,
    snd_nxt: u32,
    snd_wnd: u32,
    snd_wl1: u32,
    snd_wl2: u32,
    rcv_nxt: u32,
    retransmission_queue: Vec<Segment>,
    timer_pending: Option<Timer>,
}

enum Timer {
    Retransmission(Instant),
    TimeWait(Instant),
}

/// IP Time To Live
/// Default taken from Wikipedia
static IP_TTL: u8 = 64;
static TIMEOUT_RETR: Duration = Duration::from_secs(5);
static TIMEOUT_2MSL: Duration = Duration::from_secs(5);

// Things to do:
// Select initial sequence number
// Keep track of SND.UNA, SND.NXT, RCV.NXT
// On packet:
//  Select TCB
//  Check ACK of sent data and increment SND.UNA
//  Check seq num of incoming data and increment RCV.NXT
//
//  How to send?  TODO
//  How to retransmit? TODO

trait Service {
    fn on_connect(&mut self, data: &[u8]) -> Response;
    fn on_receive(&mut self, data: &[u8]) -> Response;
    fn on_reset(&mut self);
    fn on_close(&mut self, data: &[u8]);
}

type Socket = ([u8; 4], u16);

impl TCP {
    pub fn with_iface(
        iface: tun_tap::Iface,
        svc: Box<dyn Service>,
        local_socket: Socket,
        foreign_socket: Option<Socket>,
        open_mode: OpenMode,
    ) -> Self {
        Self {
            iface,
            buf: [0u8; 2000],
            window_size: 1000,
            tcb: TCB {
                state: TCPState::Listen,
                svc,
                local_socket,
                foreign_socket,
                open_mode,
                irs: 0,
                iss: 0,
                snd_una: 0,
                snd_nxt: 0,
                snd_wl1: 0,
                snd_wl2: 0,
                snd_wnd: 0,
                rcv_nxt: 0,
                retransmission_queue: Vec::new(),
                timer_pending: None,
            },
        }
    }

    pub fn tick(&mut self) {
        match self.tcb.timer_pending {
            Some(Timer::Retransmission(start_time)) => {
                if self.tcb.retransmission_queue.len() > 0
                    && Instant::now() - start_time > TIMEOUT_RETR
                {
                    let (iph, mut tcph, pld) =
                        self.tcb.retransmission_queue.first().unwrap().clone();
                    self.send_tcph(&mut tcph, &iph, &pld);
                }
            }
            Some(Timer::TimeWait(start_time)) => {
                if Instant::now() - start_time > TIMEOUT_2MSL {
                    self.tcb.state = TCPState::Closed;
                    self.tcb.svc.on_close(&[]);
                }
            }
            None => {}
        }

        let read = self.iface.recv(&mut self.buf).expect("Failed to read");
        let data = self.buf.clone();
        let (in_iph, in_ippld) =
            if let Ok((in_iph, pld)) = Ipv4Header::read_from_slice(&data[0..read]) {
                (in_iph, pld)
            } else {
                // eprintln!("Dropping packet: not IPv4");
                return;
            };

        if in_iph.protocol != 6 {
            // eprintln!("Dropping packet: not TCP");
            return;
        }

        let (in_tcph, in_tcppld) =
            TcpHeader::read_from_slice(in_ippld).expect("Failed to parse TCP");

        let seg_len = {
            let mut seg_len = in_tcppld.len() as u32;
            if in_tcph.syn {
                seg_len += 1;
            }
            if in_tcph.fin {
                seg_len += 1;
            }
            seg_len
        };

        eprintln!("Received segment: ");
        eprintln!("{:?}", in_tcppld);
        eprintln!("Current State: {:?}", &self.tcb.state);

        match &self.tcb.state {
            TCPState::Closed => {
                if in_tcph.rst {
                    return;
                }

                if in_tcph.ack {
                    self.reset_simple(&in_tcph, &in_iph);
                    return;
                }

                let seq = 0;
                let ack = in_tcph.sequence_number + seg_len;

                let mut out_tcph = tcph_reply(&in_tcph, seq, self.window_size);
                out_tcph.acknowledgment_number = ack;

                out_tcph.ack = true;
                out_tcph.rst = true;

                self.send_tcph(&mut out_tcph, &in_iph, &[]);
                return;
            }
            TCPState::Listen => {
                if in_tcph.rst {
                    return;
                }

                if in_tcph.ack {
                    self.reset_simple(&in_tcph, &in_iph);
                    return;
                }

                if in_tcph.syn {
                    self.tcb.rcv_nxt = in_tcph.sequence_number + 1;
                    self.tcb.irs = in_tcph.sequence_number;
                    self.tcb.iss = 123445;

                    let mut out_tcph = tcph_reply(
                        &in_tcph,
                        self.tcb.iss, // sequence_number
                        self.window_size,
                    );
                    out_tcph.acknowledgment_number = self.tcb.rcv_nxt;
                    out_tcph.syn = true;
                    out_tcph.ack = true;

                    self.send_tcph(&mut out_tcph, &in_iph, &[]);

                    self.tcb.snd_nxt = self.tcb.iss + 1;
                    self.tcb.snd_una = self.tcb.iss;

                    self.tcb.state = TCPState::SynRecvd;

                    // TODO - Fill in foreign socket
                }
                eprintln!("SynRecvd");

                // In case any segment fall through above checks,
                // drop them
                return;
            }
            TCPState::SynSent => {
                let mut is_ack_acceptable = false;
                if in_tcph.ack {
                    if in_tcph.acknowledgment_number <= self.tcb.iss
                        || in_tcph.acknowledgment_number > self.tcb.snd_nxt
                    {
                        self.reset_simple(&in_tcph, &in_iph);
                        return;
                    }

                    is_ack_acceptable = true;
                }

                if in_tcph.rst {
                    if is_ack_acceptable {
                        self.tcb.svc.on_reset();

                        self.tcb.state = TCPState::Closed;
                        return;
                    }
                    // ignore rst on unacceptable ack
                    return;
                }

                debug_assert!(is_ack_acceptable || (!in_tcph.ack && !in_tcph.rst));

                if in_tcph.syn {
                    self.tcb.rcv_nxt = in_tcph.sequence_number + 1;
                    self.tcb.irs = in_tcph.sequence_number;
                    if in_tcph.ack {
                        self.tcb.snd_una = in_tcph.acknowledgment_number;
                    }

                    // remove segments from retransmission queue
                    {
                        let snd_una = self.tcb.snd_una;
                        self.tcb.retransmission_queue.retain(|(_iph, tcph, pld)| {
                            let mut last_seq = tcph.sequence_number + (pld.len() as u32) - 1;
                            if tcph.syn {
                                last_seq += 1;
                            }
                            if tcph.fin {
                                last_seq += 1;
                            }
                            last_seq >= snd_una
                        });
                    }

                    if self.tcb.snd_una > self.tcb.iss {
                        self.tcb.state = TCPState::Estab;

                        let seq = self.tcb.snd_nxt;
                        // We don't increment SND.NXT, cos ACK doesn't occupy sequence space

                        let mut out_tcph = tcph_reply(&in_tcph, seq, self.window_size);
                        out_tcph.acknowledgment_number = self.tcb.rcv_nxt;
                        out_tcph.ack = true;
                        self.send_tcph(&mut out_tcph, &in_iph, &[]);
                        return;
                    } else {
                        self.tcb.state = TCPState::SynRecvd;
                        let seq = self.tcb.iss;
                        let mut out_tcph = tcph_reply(&in_tcph, seq, self.window_size);
                        out_tcph.acknowledgment_number = self.tcb.rcv_nxt;
                        out_tcph.syn = true;
                        out_tcph.ack = true;
                        self.send_tcph(&mut out_tcph, &in_iph, &[]);
                        return;
                    }
                }

                debug_assert!(!in_tcph.syn && !in_tcph.rst);
                // neither SYN nor RST, drop
                return;
            }
            _ => {}
        }

        // Check Sequence Number
        if [
            TCPState::SynRecvd,
            TCPState::Estab,
            TCPState::FinWait1,
            TCPState::FinWait2,
            TCPState::CloseWait,
            TCPState::Closing,
            TCPState::LastAck,
            TCPState::TimeWait,
        ]
        .contains(&self.tcb.state)
        {
            let is_seg_acceptable = {
                let case1 = seg_len == 0
                    && self.window_size == 0
                    && in_tcph.sequence_number == self.tcb.rcv_nxt;
                let case2 = seg_len == 0
                    && self.window_size > 0
                    && self.tcb.rcv_nxt <= in_tcph.sequence_number
                    && in_tcph.sequence_number < (self.tcb.rcv_nxt + self.window_size as u32);

                // Note: We are checking if part of the packet coincides with top end of receive
                // window.
                // Slightly tweaked from the original spec.
                // ie, we will drop packets if RCV.NXT octet is not in the packet.
                let case3 = seg_len > 0
                    && self.window_size > 0
                    && in_tcph.sequence_number <= self.tcb.rcv_nxt
                    && self.tcb.rcv_nxt <= in_tcph.sequence_number + seg_len - 1
                    && in_tcph.sequence_number + seg_len - 1
                        < self.tcb.rcv_nxt + self.window_size as u32;
                dbg!(case1) || dbg!(case2) || dbg!(case3)
            };
            if !is_seg_acceptable && !in_tcph.rst {
                println!("HERE");
                let seq = self.tcb.snd_nxt;
                let ack = self.tcb.rcv_nxt;
                let mut out_tcph = tcph_reply(&in_tcph, seq, self.window_size);
                out_tcph.acknowledgment_number = ack;
                out_tcph.ack = true;
                self.send_tcph(&mut out_tcph, &in_iph, &[]);
                // ack the old segment, ignore data, return
                return;
            }
        }

        // Check RST bit
        if in_tcph.rst {
            match &self.tcb.state {
                TCPState::SynRecvd => {
                    if self.tcb.open_mode == OpenMode::Passive {
                        self.tcb.state = TCPState::Listen;
                        return;
                    } else {
                        self.tcb.state = TCPState::Closed;
                        self.tcb.svc.on_close(&[]);
                        return;
                    }
                }
                TCPState::Estab | TCPState::FinWait1 | TCPState::FinWait2 | TCPState::CloseWait => {
                    self.tcb.state = TCPState::Closed;
                    self.tcb.svc.on_close(&[]);
                    return;
                }
                TCPState::Closing | TCPState::LastAck | TCPState::TimeWait => {
                    self.tcb.state = TCPState::Closed;
                    self.tcb.svc.on_close(&[]);
                    return;
                }
                _ => {}
            }
        }

        // Check ACK bit
        if !in_tcph.ack {
            // No ACK, so DROP
            return;
        }

        // ACK is set
        if TCPState::SynRecvd == self.tcb.state {
            if self.tcb.snd_una <= in_tcph.acknowledgment_number
                && in_tcph.acknowledgment_number <= self.tcb.snd_nxt
            {
                self.tcb.state = TCPState::Estab;
            // continue processing. don't return here
            } else {
                self.reset_simple(&in_tcph, &in_iph);
            }
        }
        if [
            TCPState::Estab,
            TCPState::FinWait1,
            TCPState::CloseWait,
            TCPState::Closing,
        ]
        .contains(&self.tcb.state)
        {
            if self.tcb.snd_una <= in_tcph.acknowledgment_number
                && in_tcph.acknowledgment_number <= self.tcb.snd_nxt
            {
                self.tcb.snd_una = in_tcph.acknowledgment_number;
                // remove segments from retransmission queue
                {
                    let snd_una = self.tcb.snd_una;
                    self.tcb.retransmission_queue.retain(|(_iph, tcph, pld)| {
                        let mut last_seq = tcph.sequence_number + (pld.len() as u32) - 1;
                        if tcph.syn {
                            last_seq += 1;
                        }
                        if tcph.fin {
                            last_seq += 1;
                        }
                        last_seq >= snd_una
                    });
                }

                // Update Send Window
                if self.tcb.snd_wl1 < in_tcph.sequence_number
                    || self.tcb.snd_wl1 == in_tcph.sequence_number
                        && self.tcb.snd_wl2 <= in_tcph.acknowledgment_number
                {
                    self.tcb.snd_wnd = in_tcph.window_size as u32;
                    self.tcb.snd_wl1 = in_tcph.sequence_number;
                    self.tcb.snd_wl2 = in_tcph.acknowledgment_number;
                }
            } else if in_tcph.acknowledgment_number < self.tcb.snd_una {
                // ignore
            } else if in_tcph.acknowledgment_number > self.tcb.snd_nxt {
                println!("HERE2");
                let seq = self.tcb.snd_nxt;
                let ack = self.tcb.rcv_nxt;
                let mut out_tcph = tcph_reply(&in_tcph, seq, self.window_size);
                out_tcph.acknowledgment_number = ack;
                out_tcph.ack = true;
                self.send_tcph(&mut out_tcph, &in_iph, &[]);
                // ack the old segment, ignore data, return
                return;
            }

            if self.tcb.state == TCPState::FinWait1 {
                // If our FIN was acked, enter FinWait2 and continue processing

                // FIN is the last unacknowledged
                let fin_seq = self.tcb.snd_una;
                if in_tcph.acknowledgment_number == fin_seq {
                    self.tcb.state = TCPState::FinWait2;
                }
            }

            if self.tcb.state == TCPState::FinWait2 {
                // donot delete tcb
            }

            if self.tcb.state == TCPState::Closing {
                // If our FIN was acked, enter TimeWait, else ignore segment

                // FIN is the last unacknowledged
                let fin_seq = self.tcb.snd_una;
                if in_tcph.acknowledgment_number == fin_seq {
                    self.tcb.state = TCPState::FinWait2;
                } else {
                    return;
                }
            }
        }

        if TCPState::LastAck == self.tcb.state {
            // If our FIN was acked, enter Closed state

            // FIN is the last unacknowledged
            let fin_seq = self.tcb.snd_una;
            if in_tcph.acknowledgment_number == fin_seq {
                self.tcb.state = TCPState::Closed;
            }
        }

        if TCPState::TimeWait == self.tcb.state {
            // If it is a retransmission of remote FIN,
            // ACK it and restart 2MSL timeout

            if in_tcph.fin {
                // We don't increment SND.NXT, cos ACK doesn't occupy sequence space
                let seq = self.tcb.snd_nxt;
                let ack = in_tcph.sequence_number + seg_len;

                let mut out_tcph = tcph_reply(&in_tcph, seq, self.window_size);
                out_tcph.acknowledgment_number = ack;
                out_tcph.ack = true;
                self.send_tcph(&mut out_tcph, &in_iph, &[]);

                // Restart TimeWait timeout
                self.tcb.timer_pending = Some(Timer::TimeWait(Instant::now()));
            }
        }

        // Process Segment Text
        if [TCPState::Estab, TCPState::FinWait1, TCPState::FinWait2].contains(&self.tcb.state) {
            self.tcb.rcv_nxt = in_tcph.sequence_number + seg_len;

            let seq = self.tcb.snd_nxt;
            let ack = self.tcb.rcv_nxt;
            let mut out_tcph = tcph_reply(&in_tcph, seq, self.window_size);
            out_tcph.acknowledgment_number = ack;
            out_tcph.ack = true;

            let data_to_send = if in_tcppld.len() > 0 {
                self.tcb.svc.on_receive(in_tcppld)
            } else {
                Response::None
            };

            println!("Here");
            if let Response::Data(data_to_send) = data_to_send {
                self.send_tcph(&mut out_tcph, &in_iph, &data_to_send[..]);
                self.tcb.snd_nxt += data_to_send.len() as u32;
            } else if let Response::Close(data_to_send) = data_to_send {
                out_tcph.fin = true;
                self.send_tcph(&mut out_tcph, &in_iph, &data_to_send[..]);
                self.tcb.snd_nxt += data_to_send.len() as u32;
                self.tcb.state = TCPState::FinWait1;
                return;
            } else {
                self.send_tcph(&mut out_tcph, &in_iph, &[]);
            }
        }

        if in_tcph.fin {
            if [
                TCPState::CloseWait,
                TCPState::Closing,
                TCPState::LastAck,
                TCPState::TimeWait,
            ]
            .contains(&self.tcb.state)
            {
                // Should not occur, since we got a FIN already.
                // ignore and return
                return;
            }

            // Check FIN
            if [TCPState::Closed, TCPState::Listen, TCPState::SynSent].contains(&self.tcb.state) {
                // Do not process FIN cos we can't validate SEG.SEQ
                // ignore and return
                return;
            }

            if [TCPState::SynRecvd, TCPState::Estab].contains(&self.tcb.state) {
                self.tcb.state = TCPState::CloseWait;
            }

            if TCPState::FinWait1 == self.tcb.state {
                // If our FIN was acked, enter Closed state

                // FIN is the last unacknowledged
                let fin_seq = self.tcb.snd_una;
                if in_tcph.acknowledgment_number == fin_seq {
                    self.tcb.state = TCPState::Closing;
                }
            }

            if TCPState::FinWait2 == self.tcb.state {
                self.tcb.state = TCPState::TimeWait;
                // Start time wait timer, turn off other timers
                self.tcb.timer_pending = Some(Timer::TimeWait(Instant::now()));
            }

            if TCPState::TimeWait == self.tcb.state {
                // Restart 2MSL timeout
                self.tcb.timer_pending = Some(Timer::TimeWait(Instant::now()));
            }
        }
        eprintln!("New State: {:?}", &self.tcb.state);
        eprintln!("-----");
    }

    fn reset_simple(&mut self, in_tcph: &TcpHeader, in_iph: &Ipv4Header) {
        let seq = in_tcph.acknowledgment_number;
        let mut out_tcph = tcph_reply(&in_tcph, seq, self.window_size);
        out_tcph.rst = true;

        self.send_tcph(&mut out_tcph, &in_iph, &[]);
    }

    fn send_tcph(&mut self, out_tcph: &mut TcpHeader, in_iph: &Ipv4Header, data: &[u8]) {
        let out_iph = iph_reply(&out_tcph, &in_iph, data);
        let mut buf = &mut self.buf[..];

        out_iph.write(&mut buf).unwrap();

        out_tcph.checksum = out_tcph.calc_checksum_ipv4(&out_iph, data).unwrap();
        out_tcph.write(&mut buf).unwrap();

        let mut reader = data;
        std::io::copy(&mut reader, &mut buf).unwrap();

        self.iface.send(&self.buf).unwrap();
    }
}

fn tcph_reply(in_tcph: &TcpHeader, seq_num: u32, window_size: u16) -> TcpHeader {
    TcpHeader::new(
        in_tcph.destination_port, // Incoming destination is our source
        in_tcph.source_port,      // Incoming source is our dest
        seq_num,                  // Sequence Number.
        window_size,              // Window size.
    )
}

fn iph_reply(out_tcph: &TcpHeader, in_iph: &Ipv4Header, data: &[u8]) -> Ipv4Header {
    Ipv4Header::new(
        out_tcph.header_len() + data.len() as u16, // Payload length
        IP_TTL,                                    // Time to live.
        IpTrafficClass::Tcp,                       // Protocol
        in_iph.destination,                        // Incoming destination is our source
        in_iph.source,                             // Incoming source is our dest
    )
}
