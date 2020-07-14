#![allow(dead_code)]

use etherparse::{IpTrafficClass, Ipv4Header, TcpHeader};

fn main() {
    let mut tcp = {
        let iface = tun_tap::Iface::without_packet_info("", tun_tap::Mode::Tun)
            .expect("Failed to initialize TUN interface");
        TCP::with_iface(iface)
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

enum TCPState {
    Listen,
    SynRecvd { initial_sequence_number: u32 },
    Estab,
}

struct TCB {
    state: TCPState,
    local_socket: ([u8; 4], u16),
    foreign_socket: Option<([u8; 4], u16)>,
    snd_una: u32,
    snd_nxt: u32,
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

impl TCP {
    pub fn with_iface(iface: tun_tap::Iface) -> Self {
        Self {
            iface,
            buf: [0u8; 2000],
            window_size: 1000,
            tcb: TCB {
                state: TCPState::Listen,
                local_socket: ([10, 0, 0, 1], 1000),
                foreign_socket: None,
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

        eprintln!("Received segment: ");
        dbg!(&in_iph);
        dbg!((&in_tcph, &in_tcppld));

        match &self.tcb.state {
            TCPState::Listen => {
                // Waiting for connection:
                // Incoming packet must be SYN.
                if !in_tcph.syn {
                    eprintln!("Dropping seg: state LISTEN, seg not SYN");
                    return;
                }

                // ACK the SYN, send our SYN

                let initial_sequence_number = 123445; // FIXME

                let mut out_tcph = tcph_reply(&in_tcph, initial_sequence_number, self.window_size);
                out_tcph.syn = true;
                out_tcph.ack = true;
                out_tcph.acknowledgment_number = in_tcph.sequence_number + 1;

                let out_iph = iph_reply(&out_tcph, &in_iph);
                let mut buf = &mut self.buf[..];

                out_iph.write(&mut buf).unwrap();

                out_tcph.checksum = out_tcph.calc_checksum_ipv4(&out_iph, &[]).unwrap();
                out_tcph.write(&mut buf).unwrap();

                self.iface.send(&self.buf).unwrap();

                self.tcb.state = TCPState::SynRecvd {
                    initial_sequence_number,
                };

                eprintln!("SynRecvd");
            }
            TCPState::SynRecvd {
                initial_sequence_number,
            } => {
                if !in_tcph.ack {
                    eprintln!("Dropping seg: state SynRecvd, seg not ACK");
                    return;
                }

                if !in_tcph.acknowledgment_number == initial_sequence_number + 1 {
                    eprintln!("Dropping seg: state SynRecvd, ack num mismatch.");
                    eprintln!(
                        "Expected: {} Got: {}",
                        initial_sequence_number + 1,
                        in_tcph.acknowledgment_number
                    );
                }

                self.tcb.state = TCPState::Estab;

                eprintln!("Established :)");
            }
            TCPState::Estab => unimplemented!(),
        }
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
