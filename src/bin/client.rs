use std::io::{Read, Write};
use std::io::{stdin, stdout};
use std::net::{SocketAddr, TcpStream};

const BUFSIZE: usize = 4096;
const PORTNUM: u16 = 8000;

const GET_MSG: &str = "FILE(";
#[allow(unused)]
const LS_MSG: &str = "LIST(";

type Error = Box<dyn std::error::Error>;

fn parse_size_header(s: &str, prefix: &str) -> Option<usize> {
    let rest = s.strip_prefix(prefix)?;
    let (size_str, _) = rest.split_once("): ")?;
    size_str.parse::<usize>().ok()
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
        let response = std::str::from_utf8(&response_bytes[..n]).expect("Invalid UTF-8");
        print!("{response}");
        stdout().flush()?;

        if response.starts_with(GET_MSG)
            && let Some(mut size) = parse_size_header(response, GET_MSG)
        {
            while size > 0 {
                let n = stream.read(&mut response_bytes[..size.min(BUFSIZE)])?;
                if n == 0 {
                    break;
                }
                stdout().write_all(&response_bytes[..n])?;
                size -= size.min(n);
            }
        }
    }

    Ok(())
}
