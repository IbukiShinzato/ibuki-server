use std::env;
use std::fs::{self, File, remove_file};
use std::io::{ErrorKind, Read, Write, stdout};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Component, Path};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

pub type Error = Box<dyn std::error::Error>;

const INADDR_ANY: [u8; 4] = [0, 0, 0, 0];

const PORT: u16 = 8000;
const BUFSIZE: usize = 4096;
const MAXCLIENTS: usize = 20;

const GET_PREFIX: &str = "GET<";
const PUT_PREFIX: &str = "PUT<";
const DEL_PREFIX: &str = "DEL<";
const LS_PREFIX: &str = "LS";

enum Protocol {
    Get,
    Put,
    Del,
    Ls,
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

fn delete_file(stream: &mut TcpStream, filename: &str) -> Result<(), Error> {
    if !is_valid_filename(filename) {
        stream.write_all(b"INVALID PATH NAME\n")?;
        return Ok(());
    }

    let Some(home) = env::home_dir() else {
        stream.write_all(b"SERVER ERROR\n")?;
        return Ok(());
    };

    let path = home.join(filename);

    match remove_file(path) {
        Ok(()) => (),
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

    writeln!(stream, "DEL: {filename} deleted")?;

    Ok(())
}

fn list_files(stream: &mut TcpStream) -> Result<(), Error> {
    let Some(home) = env::home_dir() else {
        stream.write_all(b"SERVER ERROR\n")?;
        return Ok(());
    };

    let entries = fs::read_dir(home)?
        .map(|entry| {
            let entry = entry?;
            Ok(entry.file_name().to_string_lossy().into_owned())
        })
        .collect::<Result<Vec<String>, Error>>()?;

    let entries: Vec<String> = entries
        .into_iter()
        .filter(|name| !name.starts_with("."))
        .collect();

    let body = entries.join("\n") + "\n";
    let header = format!("LIST({}): ", body.len());

    stream.write_all(header.as_bytes())?;
    stream.write_all(body.as_bytes())?;

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

    if msg.starts_with(DEL_PREFIX) {
        return Protocol::Del;
    }

    if msg == LS_PREFIX {
        return Protocol::Ls;
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
        Protocol::Del => {
            if let Some((filename, rest)) = parse_angle_brackets(msg, DEL_PREFIX)
                && rest.is_empty()
            {
                delete_file(stream, filename)?;
            } else {
                stream.write_all(b"PROTOCOL ERROR\n")?;
            }
        }
        Protocol::Ls => {
            list_files(stream)?;
        }
        Protocol::Error => {
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

    let num_of_threads = Arc::new((Mutex::new(0), Condvar::new()));

    loop {
        let (mut stream, _addr) = listener.accept()?;

        {
            let (lock, cvar) = &*num_of_threads;
            let mut count = lock.lock().unwrap();

            while *count >= MAXCLIENTS {
                count = cvar.wait(count).unwrap();
            }

            *count += 1;
        }

        let num_of_threads = Arc::clone(&num_of_threads);

        thread::spawn(move || {
            match recv_and_resp(&mut stream) {
                Ok(_) => (),
                Err(e) => {
                    eprintln!("{e}");
                }
            }

            let (lock, cvar) = &*num_of_threads;
            let mut count = lock.lock().unwrap();
            *count -= 1;
            cvar.notify_one();
        });
    }
}
