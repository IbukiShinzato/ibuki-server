use std::fs::File;
use std::io::{Read, Write, stdout};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::{env, fs};

pub type Error = Box<dyn std::error::Error>;

const INADDR_ANY: [u8; 4] = [0, 0, 0, 0];

const PORT: u16 = 8000;
const BUFSIZE: usize = 4096;

const GET_PREFIX: &str = "GET<";

fn send_file(stream: &mut TcpStream, filename: String) -> Result<(), Error> {
    if filename.is_empty() || filename.starts_with("/") || filename.contains("..") {
        stream.write_all(b"INVALID PATH NAME\n")?;
        return Ok(());
    }

    if let Some(mut home) = env::home_dir() {
        home.push(filename);
        let size = match fs::metadata(&home) {
            Ok(metadata) => metadata.len(),
            Err(_) => {
                stream.write_all(b"NOT FOUND\n")?;
                return Ok(());
            }
        };
        let mut file = match File::open(home) {
            Ok(file) => file,
            Err(_) => {
                stream.write_all(b"NOT FOUND\n")?;
                return Ok(());
            }
        };
        let mut buf = [0; BUFSIZE];

        let header = format!("FILE({size}): ");
        let header_bytes = header.as_bytes();
        stream.write_all(header_bytes)?;

        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }

            stream.write_all(&buf[..n])?;
        }
    }

    Ok(())
}

fn parse_get_filename<'a>(s: &'a str, prefix: &str) -> Option<(&'a str, &'a str)> {
    let rest = s.strip_prefix(prefix)?;
    rest.split_once(">")
}

fn resp_msg(stream: &mut TcpStream, msg: &str) -> Result<(), Error> {
    if let Some((filename, rest)) = parse_get_filename(msg, GET_PREFIX) {
        if !rest.is_empty() {
            stream.write_all(b"PROTOCOL ERROR\n")?;
            return Ok(());
        }

        send_file(stream, filename.to_string())?;

        return Ok(());
    }

    stream.write_all(b"PROTOCOL ERROR\n")?;

    Ok(())
}

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
            resp_msg(stream, dst.as_str())?;
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

        thread::spawn(move || match recv_and_resp(&mut stream) {
            Ok(_) => (),
            Err(e) => {
                eprintln!("{e}");
            }
        });
    }
}
