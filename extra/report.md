---
mainfont: Fira Sans Regular
geometry: margin=1in
---
# Networks Mini Project Report

## Minimally conforming TCP implementation in userspace.
## Muhammed Farseen CK B170815CS


This project is a minimal TCP stack, which can be used in embedded devices and
other resource constrained settings. 

It implements a subset of the functional specification outlined in RFC 793. 

What is implemented:

* TCP State machine
* Passive Open
* Connection establishment and clearing
* Data transfer
* Retransmission

What is not implemented:

* Active Open
* Flow Control
* Urgent Pointer
* Precedence and Security

It is written in the systems programming language Rust, which can target a
variety of embedded boards while providing high level features not found in C.

Since it is intended to work in a resource constrained environment, it is
implemented as single threaded. Therefore, it can only serve one client
simultaneously. But it can serve more than one client sequentially as first
client closes the connection. So it is suited to short lived connections like in
an HTTP Web Server for an IoT device. A demo HTTP Server is also included as an
example, which supports a minimal subset of HTTP 1.0.

The main goal of this project has been to understand the internal workings of
the TCP protocol. It was a really good learning experience to read the original
specification and implementing and debugging it. All of the functionality is
implemented from scratch, without using the corresponding OS functionality.

Interfacing with lower layer is currently done using Linux TUN driver.
But it can be easily ported to any IP implementation providing a send() and
a receive() function as shown below.

```
struct IPPDU {
    IpHeader hdr;
    byte[] payload;
}

interface IP {
    void send(IPPDU packet);
    IPPDU receive();
}
```

The TCP accepts any application layer object which implements the following functions:
```
interface ApplicationService {
    Response on_connect(data);
    Response on_receive(data);
    void on_close();
}

struct Response {
    byte[] reply;
    bool should_close;
}

where
    reply: Array of response data. Can be empty if no response needs to be sent
    should_close: Whether to close the connection after sending this
    response
```

The example code provides implementations for an Echo server and HTTP Server.
