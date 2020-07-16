mod tcp;
use tcp::*;

mod http_server;
use http_server::*;

mod echo_server;
use echo_server::*;

fn main() {
    let mut iface = tun_tap::Iface::without_packet_info("", tun_tap::Mode::Tun)
        .expect("Failed to initialize TUN interface");

    let local_socket = ([10, 0, 0, 2], 1000);
    let foreign_socket = None;

    println!("Welcome to TCP demo");
    println!("Choose which server to run: ");
    println!("1. Echo Server");
    println!("2. HTTP Server");
    println!("Enter your choice: ");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    let input = input.trim();

    let mut svc: Box<dyn Service> = if input == "1" {
        Box::new(EchoServer)
    } else if input == "2" {
        Box::new(HTTPServer)
    } else {
        println!("Invalid choice");
        return;
    };

    loop {
        let mut tcp = {
            TCP::with_iface(
                iface,
                svc,
                local_socket,
                foreign_socket,
                OpenMode::Passive,
            )
        };

        while tcp.tcb.state != TCPState::Closed {
            tcp.tick();
        }

        iface = tcp.iface;
        svc = tcp.tcb.svc; 
    }
}
