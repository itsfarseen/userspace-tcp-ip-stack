use crate::tcp::Service;
use crate::tcp::Response;

pub struct HTTPServer;

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
