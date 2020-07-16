use crate::tcp::Service;
use crate::tcp::Response;

pub struct EchoServer;
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
