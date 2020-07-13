pub struct TcpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub seq_num: u32,
    pub ack_num: u32,
    pub data_offset: u8,
    pub urg: bool,
    pub ack: bool,
    pub psh: bool,
    pub rst: bool,
    pub syn: bool,
    pub fin: bool,
    pub window: u16,
    pub checksum: u16,
    pub urg_ptr: u16,
    pub options: Vec<TcpOption>,
    pub data: Vec<u8>
}

pub enum TcpOption {
    MSS(u16)
}

impl TcpHeader {
    pub fn from_bytes(buf: &[u8]) {
       let src_port = u16_from_be_bytes(buf, 0);
       let dst_port = u16_from_be_bytes(buf, 2);
       let seq_num = u32_from_be_bytes(buf, 4);
       let ack_num = u32_from_be_bytes(buf, 8);
       let data_offset = buf[12] >> 4;
       let urg = buf[13] & 0b00100000 > 0;
       let ack = buf[13] & 0b00010000 > 0;
       let psh = buf[13] & 0b00001000 > 0;
       let rst = buf[13] & 0b00000100 > 0;
       let syn = buf[13] & 0b00000010 > 0;
       let fin = buf[13] & 0b00000001 > 0;
       let window = u16_from_be_bytes(buf, 14);
       let checksum = u16_from_be_bytes(buf, 16);
       let urg_ptr = u16_from_be_bytes(buf, 18);
    }
}

fn u16_from_be_bytes(buf: &[u8], idx: usize) -> u16 {
    let mut buf1: [u8; 2] = Default::default();
    buf1.copy_from_slice(&buf[idx..idx+2]);
    u16::from_be_bytes(buf1)
}

fn u32_from_be_bytes(buf: &[u8], idx: usize) -> u32 {
    let mut buf1: [u8; 4] = Default::default();
    buf1.copy_from_slice(&buf[idx..idx+4]);
    u32::from_be_bytes(buf1)
}
