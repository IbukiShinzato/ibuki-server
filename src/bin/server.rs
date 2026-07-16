use std::fs::File;
use std::io::ErrorKind;
use std::io::{Read, Write, stdout};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Component, Path};
use std::thread;
use std::{env, fs};

pub type Error = Box<dyn std::error::Error>;

const INADDR_ANY: [u8; 4] = [0, 0, 0, 0];

const PORT: u16 = 8000;
const BUFSIZE: usize = 4096;

const GET_PREFIX: &str = "GET<";
const PUT_PREFIX: &str = "PUT<";

enum Protocol {
    Get,
    Put,
    Error,
}

fn is_valid_filename(filename: &str) -> bool {
    let mut components = Path::new(filename).components();

    matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(_)), None)
    )
}

fn send_file(stream: &mut TcpStream, filename: &str) -> Result<(), Error> {
    if !is_valid_filename(filename) {
        stream.write_all(b"INVALID PATH NAME\n")?;
        return Ok(());
    }

    let Some(home) = env::home_dir() else {
        stream.write_all(b"SERVER ERROR\n")?;
        return Ok(());
    };

    let path = home.join(filename);

    let size = match fs::metadata(&path) {
        Ok(metadata) => metadata.len(),
        Err(_) => {
            stream.write_all(b"NOT FOUND\n")?;
            return Ok(());
        }
    };
    let mut file = match File::open(path) {
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

    Ok(())
}

fn save_file(stream: &mut TcpStream, filename: &str, content: &str) -> Result<(), Error> {
    if !is_valid_filename(filename) {
        stream.write_all(b"INVALID PATH NAME\n")?;
        return Ok(());
    }

    let Some(home) = env::home_dir() else {
        stream.write_all(b"SERVER ERROR\n")?;
        return Ok(());
    };

    let path = home.join(filename);

    let mut file = match File::create(&path) {
        Ok(file) => file,
        Err(error) => {
            let response: &[u8] = match error.kind() {
                ErrorKind::PermissionDenied => b"NOT PERMISSION\n",
                ErrorKind::NotFound => b"NOT FOUND\n",
                _ => b"FILE ERROR\n",
            };

            stream.write_all(response)?;
            return Ok(());
        }
    };

    file.write_all(content.as_bytes())?;
    writeln!(stream, "PUT: {filename} saved")?;

    Ok(())
}

fn parse_angle_brackets<'a>(msg: &'a str, prefix: &str) -> Option<(&'a str, &'a str)> {
    let rest = msg.strip_prefix(prefix)?;
    rest.split_once(">")
}

fn parse_protocol(msg: &str) -> Protocol {
    if msg.starts_with(GET_PREFIX) {
        return Protocol::Get;
    }

    if msg.starts_with(PUT_PREFIX) {
        return Protocol::Put;
    }

    Protocol::Error
}

fn resp_msg(stream: &mut TcpStream, msg: &str) -> Result<(), Error> {
    let protocol = parse_protocol(msg);

    match protocol {
        Protocol::Get => {
            if let Some((filename, rest)) = parse_angle_brackets(msg, GET_PREFIX)
                && rest.is_empty()
            {
                send_file(stream, filename)?;
            } else {
                stream.write_all(b"PROTOCOL ERROR\n")?;
            };
        }
        Protocol::Put => {
            if let Some((filename, rest)) = parse_angle_brackets(msg, PUT_PREFIX)
                && let Some((content, rest)) = parse_angle_brackets(rest, "<")
                && rest.is_empty()
            {
                save_file(stream, filename, content)?;
            } else {
                stream.write_all(b"PROTOCOL ERROR\n")?;
            }
        }
        _ => {
            stream.write_all(b"PROTOCOL ERROR\n")?;
            return Ok(());
        }
    };

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
