fn main() {
    let iface = tun_tap::Iface::without_packet_info("", tun_tap::Mode::Tun)
        .expect("Failed to initialize TUN interface");
    let mut buf = [0u8; 2000];
    loop {
        let read = iface.recv(&mut buf).expect("Failed to read");
        let (iph, ippld) =
            if let Ok((iph, pld)) = etherparse::Ipv4Header::read_from_slice(&buf[0..read]) {
                (iph, pld)
            } else {
                continue;
            };
        
        let (tcph, tcppld) =
            if let Ok((tcph, tcppld)) = etherparse::TcpHeader::read_from_slice(ippld) {
                (tcph, tcppld)
            } else {
                continue;
            };

        dbg!((tcph, tcppld));
    }
}
