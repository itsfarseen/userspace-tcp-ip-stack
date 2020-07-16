#![allow(dead_code)]

use etherparse::{IpTrafficClass, Ipv4Header, TcpHeader};

fn main() {
    let mut tcp = {
        let iface = tun_tap::Iface::without_packet_info("", tun_tap::Mode::Tun)
            .expect("Failed to initialize TUN interface");
        TCP::with_iface(iface, unimplemented!())
    };

    loop {
        tcp.tick();
    }
}

struct TCP {
    iface: tun_tap::Iface,
    buf: [u8; 2000],
    window_size: u16,
    tcb: TCB,
}

#[derive(Eq, PartialEq)]
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
}

/// IP Time To Live
/// Default taken from Wikipedia
static IP_TTL: u8 = 64;

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
    fn on_connect(&mut self);
    fn on_receive(&mut self, data: &[u8]);
}

impl TCP {
    pub fn with_iface(iface: tun_tap::Iface, svc: Box<dyn Service>) -> Self {
        Self {
            iface,
            buf: [0u8; 2000],
            window_size: 1000,
            tcb: TCB {
                state: TCPState::Listen,
                svc,
                local_socket: ([10, 0, 0, 1], 1000),
                foreign_socket: None,
                irs: 0,
                iss: 0,
                snd_una: 0,
                snd_nxt: 0,
                rcv_nxt: 0,
            },
        }
    }

    pub fn tick(&mut self) {
        let read = self.iface.recv(&mut self.buf).expect("Failed to read");
        let (in_iph, in_ippld) =
            if let Ok((in_iph, pld)) = Ipv4Header::read_from_slice(&self.buf[0..read]) {
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
        dbg!(&in_iph);
        dbg!((&in_tcph, &in_tcppld));

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

                self.send_tcph(&mut out_tcph, &in_iph);
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

                    self.send_tcph(&mut out_tcph, &in_iph);

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
                        // TODO signal user connection closed
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

                    // TODO remove segments from retransmission queue.

                    if self.tcb.snd_una > self.tcb.iss {
                        self.tcb.state = TCPState::Estab;

                        let seq = self.tcb.snd_nxt;
                        // We don't increment SND.NXT, cos ACK doesn't occupy sequence space

                        let mut out_tcph = tcph_reply(&in_tcph, seq, self.window_size);
                        out_tcph.acknowledgment_number = self.tcb.rcv_nxt;
                        out_tcph.ack = true;
                        self.send_tcph(&mut out_tcph, &in_iph);
                        return;
                    } else {
                        self.tcb.state = TCPState::SynRecvd;
                        let seq = self.tcb.iss;
                        let mut out_tcph = tcph_reply(&in_tcph, seq, self.window_size);
                        out_tcph.acknowledgment_number = self.tcb.rcv_nxt;
                        out_tcph.syn = true;
                        out_tcph.ack = true;
                        self.send_tcph(&mut out_tcph, &in_iph);
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
                let case3 = seg_len > 0
                    && self.window_size > 0
                    && self.tcb.rcv_nxt <= in_tcph.sequence_number + seg_len - 1
                    && in_tcph.sequence_number + seg_len - 1
                        < self.tcb.rcv_nxt + self.window_size as u32;
                case1 || case2 || case3
            };
            if !is_seg_acceptable && !in_tcph.rst {
                let seq = self.tcb.snd_nxt;
                let ack = self.tcb.rcv_nxt;
                let mut out_tcph = tcph_reply(&in_tcph, seq, self.window_size);
                out_tcph.acknowledgment_number = ack;
                out_tcph.ack = true;
                self.send_tcph(&mut out_tcph, &in_iph);
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
                        // TODO: Signal connection refused
                        return;
                    }
                }
                TCPState::Estab | TCPState::FinWait1 | TCPState::FinWait2 | TCPState::CloseWait => {
                    self.tcb.state = TCPState::Closed;
                    // TODO: Signal connection refused
                    return;
                }
                TCPState::Closing | TCPState::LastAck | TCPState::TimeWait => {
                    self.tcb.state = TCPState::Closed;
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
                // TODO Remove acked stuff from retransmission queue
                // TODO Signal users that data is sent

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
                let seq = self.tcb.snd_nxt;
                let ack = self.tcb.rcv_nxt;
                let mut out_tcph = tcph_reply(&in_tcph, seq, self.window_size);
                out_tcph.acknowledgment_number = ack;
                out_tcph.ack = true;
                self.send_tcph(&mut out_tcph, &in_iph);
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
                // TODO if retransmission queue is empty, reply okay to user's close
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
                    self.send_tcph(&mut out_tcph, &in_iph);

                    // TODO Retransmit 2MSL
                }
        }

        // Process Segment Text

    }

    fn reset_simple(&mut self, in_tcph: &TcpHeader, in_iph: &Ipv4Header) {
        let seq = in_tcph.acknowledgment_number;
        let mut out_tcph = tcph_reply(&in_tcph, seq, self.window_size);
        out_tcph.rst = true;

        self.send_tcph(&mut out_tcph, &in_iph);
    }

    fn send_tcph(&mut self, out_tcph: &mut TcpHeader, in_iph: &Ipv4Header) {
        let out_iph = iph_reply(&out_tcph, &in_iph);
        let mut buf = &mut self.buf[..];

        out_iph.write(&mut buf).unwrap();

        out_tcph.checksum = out_tcph.calc_checksum_ipv4(&out_iph, &[]).unwrap();
        out_tcph.write(&mut buf).unwrap();

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

fn iph_reply(out_tcph: &TcpHeader, in_iph: &Ipv4Header) -> Ipv4Header {
    Ipv4Header::new(
        out_tcph.header_len(), // Payload length
        IP_TTL,                // Time to live.
        IpTrafficClass::Tcp,   // Protocol
        in_iph.destination,    // Incoming destination is our source
        in_iph.source,         // Incoming source is our dest
    )
}
