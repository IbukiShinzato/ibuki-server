use std::io::{Read, Write, stdout};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread;

pub type Error = Box<dyn std::error::Error>;

const INADDR_ANY: [u8; 4] = [0, 0, 0, 0];

const PORT: u16 = 8000;
const BUFSIZE: usize = 4096;

fn resp_msg(_stream: &mut TcpStream, _msg: &str) {}

fn is_line(buf: &[u8]) -> Option<usize> {
    buf.iter().position(|&b| b == b'\n')
}

fn recv_and_resp(stream: &mut TcpStream) -> Result<bool, Error> {
    let mut buf = [0; BUFSIZE];
    let mut dst = String::new();

    loop {
        let ret = stream.read(&mut buf)?;
        if ret == 0 {
            return Ok(false);
        }

        let mut start = 0;

        while let Some(pos) = is_line(&buf[start..ret]) {
            dst += str::from_utf8(&buf[start..start + pos])?;
            resp_msg(stream, dst.as_str());
            dst.clear();

            start += pos + 1;
        }

        if start < ret {
            dst += str::from_utf8(&buf[start..ret])?;
        }

        print!("recv: ");
        stdout().write_all(&buf[..ret])?;
    }
}

pub fn main() -> Result<(), Error> {
    let addr = SocketAddr::from((INADDR_ANY, PORT));
    let listener = TcpListener::bind(addr)?;

    loop {
        let (mut stream, _addr) = listener.accept()?;

        thread::spawn(move || {
            loop {
                match recv_and_resp(&mut stream) {
                    Ok(true) => continue,
                    Ok(false) => break,
                    Err(e) => {
                        eprintln!("{e}");
                        break;
                    }
                }
            }
        });
    }
}
