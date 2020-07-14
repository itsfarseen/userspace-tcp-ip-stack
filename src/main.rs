use etherparse::{IpTrafficClass, Ipv4Header, TcpHeader};

fn main() {
    let mut tcp = {
        let iface = tun_tap::Iface::without_packet_info("", tun_tap::Mode::Tun)
            .expect("Failed to initialize TUN interface");
        TCP::with_iface(iface)
    };

    tcp.run();
}

struct TCP {
    iface: tun_tap::Iface,
    buf: [u8; 2000],
}

impl TCP {
    pub fn with_iface(iface: tun_tap::Iface) -> Self {
        Self {
            iface,
            buf: [0u8; 2000],
        }
    }

    pub fn run(&mut self) {
        loop {
            let read = self.iface.recv(&mut self.buf).expect("Failed to read");
            let (iph, ippld) =
                if let Ok((iph, pld)) = Ipv4Header::read_from_slice(&self.buf[0..read]) {
                    (iph, pld)
                } else {
                    // eprintln!("Dropping packet: not IPv4");
                    continue;
                };

            if iph.protocol != 6 {
                // eprintln!("Dropping packet: not TCP");
                continue;
            }

            let (tcph, tcppld) = TcpHeader::read_from_slice(ippld).expect("Failed to parse TCP");

            eprintln!("Received segment: ");
            dbg!(&iph);
            dbg!((&tcph, &tcppld));

            // Waiting for connection:
            // Incoming packet must be SYN.
            if !tcph.syn {
                eprintln!("Dropping seg: state LISTEN, seg not SYN");
                continue;
            }

            // ACK the SYN, send our SYN
            let mut tcph1 = TcpHeader::new(
                tcph.destination_port, // Incoming destination is our source
                tcph.source_port,      // Incoming source is our destt
                123445,                     // Sequence Number. FIXME
                1000,                  // Window size. FIXME
            );

            tcph1.syn = true;
            tcph1.ack = true;
            tcph1.acknowledgment_number = tcph.sequence_number + 1;

            let iph1 = Ipv4Header::new(
                tcph1.header_len(),  // Payload length
                64,                  // Time to live. (Default taken from wikipedia)
                IpTrafficClass::Tcp, // Protocol
                iph.destination,
                iph.source,
            );

            let mut buf = &mut self.buf[..];

            iph1.write(&mut buf).unwrap();

            tcph1.checksum = tcph1.calc_checksum_ipv4(&iph1, &[]).unwrap();
            tcph1.write(&mut buf).unwrap();

            self.iface.send(&self.buf).unwrap();
        }
    }
}
