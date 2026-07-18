use std::io::{self, ErrorKind, Read, Write};
use std::io::{stdin, stdout};
use std::net::{SocketAddr, TcpStream};

const BUFSIZE: usize = 4096;
const PORTNUM: u16 = 8000;

const FILE_MSG: &str = "FILE(";
const LIST_MSG: &str = "LIST(";

type Error = Box<dyn std::error::Error>;

fn find_bytes(buf: &[u8], pattern: &[u8]) -> Option<usize> {
    buf.windows(pattern.len())
        .position(|window| window == pattern)
}

fn parse_size_header(buf: &[u8], prefix: &str) -> Option<(usize, usize)> {
    let prefix = prefix.as_bytes();

    if !buf.starts_with(prefix) {
        return None;
    }

    let header_end = find_bytes(buf, b"): ")?;

    let size_bytes = &buf[prefix.len()..header_end];
    let size_str = std::str::from_utf8(size_bytes).ok()?;
    let size = size_str.parse::<usize>().ok()?;

    let body_start = header_end + b"): ".len();

    Some((size, body_start))
}

fn read_more(
    stream: &mut TcpStream,
    received: &mut Vec<u8>,
    response_bytes: &mut [u8; BUFSIZE],
) -> Result<(), Error> {
    let n = stream.read(response_bytes)?;

    if n == 0 {
        return Err(io::Error::new(
            ErrorKind::UnexpectedEof,
            "connection closed while reading response",
        )
        .into());
    }

    received.extend_from_slice(&response_bytes[..n]);

    Ok(())
}

fn read_and_output(
    stream: &mut TcpStream,
    mut size: usize,
    response_bytes: &mut [u8; BUFSIZE],
) -> Result<(), Error> {
    while size > 0 {
        let n = stream.read(&mut response_bytes[..size.min(BUFSIZE)])?;

        if n == 0 {
            return Err(io::Error::new(
                ErrorKind::UnexpectedEof,
                "connection closed while reading body",
            )
            .into());
        }

        stdout().write_all(&response_bytes[..n])?;
        size -= n;
    }

    Ok(())
}

fn handle_sized_response(
    stream: &mut TcpStream,
    mut received: Vec<u8>,
    prefix: &str,
    response_bytes: &mut [u8; BUFSIZE],
) -> Result<(), Error> {
    while parse_size_header(&received, prefix).is_none() {
        read_more(stream, &mut received, response_bytes)?;
    }

    let Some((size, body_start)) = parse_size_header(&received, prefix) else {
        return Err(io::Error::new(ErrorKind::InvalidData, "invalid sized header").into());
    };

    stdout().write_all(&received[..body_start])?;

    let already_received_body = received.len() - body_start;
    let body_len = already_received_body.min(size);

    if body_len > 0 {
        stdout().write_all(&received[body_start..body_start + body_len])?;
    }

    let remaining = size - body_len;
    read_and_output(stream, remaining, response_bytes)?;

    stdout().flush()?;

    Ok(())
}

fn handle_line_response(
    stream: &mut TcpStream,
    mut received: Vec<u8>,
    response_bytes: &mut [u8; BUFSIZE],
) -> Result<(), Error> {
    loop {
        if let Some(pos) = received.iter().position(|&b| b == b'\n') {
            stdout().write_all(&received[..=pos])?;
            stdout().flush()?;
            return Ok(());
        }

        read_more(stream, &mut received, response_bytes)?;
    }
}

fn main() -> Result<(), Error> {
    let addr = SocketAddr::from(([127, 0, 0, 1], PORTNUM));
    let mut stream = TcpStream::connect(addr)?;

    let mut buf = String::new();
    let mut response_bytes = [0; BUFSIZE];

    loop {
        buf.clear();

        print!("> ");
        stdout().flush()?;

        if stdin().read_line(&mut buf)? == 0 {
            break;
        }

        stream.write_all(buf.as_bytes())?;

        let n = stream.read(&mut response_bytes)?;

        if n == 0 {
            break;
        }

        let received = response_bytes[..n].to_vec();

        if received.starts_with(FILE_MSG.as_bytes()) {
            handle_sized_response(&mut stream, received, FILE_MSG, &mut response_bytes)?;
        } else if received.starts_with(LIST_MSG.as_bytes()) {
            handle_sized_response(&mut stream, received, LIST_MSG, &mut response_bytes)?;
        } else {
            handle_line_response(&mut stream, received, &mut response_bytes)?;
        }
    }

    Ok(())
}
