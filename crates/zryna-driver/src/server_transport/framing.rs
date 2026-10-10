//! One narrow HTTP/1.1 frame. No transfer coding, upgrades, pipelining or keepalive.

use super::{Config, Error, io::Socket};

pub(super) struct Request {
    pub(super) method: String,
    pub(super) path: String,
    pub(super) body: Vec<u8>,
}

pub(super) fn read(socket: &mut Socket, config: Config) -> Result<Request, Error> {
    let mut header = Vec::with_capacity(config.header_bytes);
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() == config.header_bytes {
            return Err(Error::Limit);
        }
        let mut byte = [0];
        if socket.read(&mut byte)? == 0 {
            return Err(Error::Framing);
        }
        if byte[0] == b'\n' && header.last() != Some(&b'\r') {
            return Err(Error::Framing);
        }
        header.push(byte[0]);
    }
    let (method, path, length) = parse(&header, config)?;
    // Header bytes are destroyed before allocating the body. Metadata copies are <=260 bytes.
    drop(header);
    let mut body = vec![0; length];
    let mut read = 0;
    while read < length {
        let count = socket.read(&mut body[read..])?;
        if count == 0 {
            return Err(Error::Framing);
        }
        read += count;
    }
    if socket.has_extra()? {
        return Err(Error::Framing);
    }
    Ok(Request { method, path, body })
}

fn parse(bytes: &[u8], config: Config) -> Result<(String, String, usize), Error> {
    let header = std::str::from_utf8(bytes).map_err(|_| Error::Framing)?;
    let mut lines = header.strip_suffix("\r\n\r\n").ok_or(Error::Framing)?.split("\r\n");
    let mut request = lines.next().ok_or(Error::Framing)?.split(' ');
    let method = request.next().ok_or(Error::Framing)?;
    let path = request.next().ok_or(Error::Framing)?;
    if request.next() != Some("HTTP/1.1")
        || request.next().is_some()
        || !matches!(method, "GET" | "POST")
        || !path.starts_with('/')
        || path.len() > 256
        || path.bytes().any(|byte| !(0x21..=0x7e).contains(&byte) || byte == b'#')
    {
        return Err(Error::Framing);
    }
    let mut names: Vec<&str> = Vec::with_capacity(32);
    let mut length = None;
    let mut host = false;
    let mut close = false;
    for line in lines {
        let (name, value) = line.split_once(':').ok_or(Error::Framing)?;
        if name.is_empty()
            || name.bytes().any(|byte| !byte.is_ascii_alphanumeric() && byte != b'-')
            || value.bytes().any(|byte| !(0x20..=0x7e).contains(&byte))
        {
            return Err(Error::Framing);
        }
        if names.len() == 32 {
            return Err(Error::Limit);
        }
        if names.iter().any(|existing| existing.eq_ignore_ascii_case(name)) {
            return Err(Error::Framing);
        }
        names.push(name);
        let value = value.trim_matches(' ');
        match name {
            name if name.eq_ignore_ascii_case("host") => {
                if value != config.address.to_string() {
                    return Err(Error::Framing);
                }
                host = true;
            }
            name if name.eq_ignore_ascii_case("content-length") => {
                if value.is_empty()
                    || value.len() > 10
                    || !value.bytes().all(|byte| byte.is_ascii_digit())
                    || (value.len() > 1 && value.starts_with('0'))
                {
                    return Err(Error::Framing);
                }
                let count = value.parse::<usize>().map_err(|_| Error::Limit)?;
                if count > config.body_bytes {
                    return Err(Error::Limit);
                }
                length = Some(count);
            }
            name if name.eq_ignore_ascii_case("connection") => {
                if !value.eq_ignore_ascii_case("close") {
                    return Err(Error::Framing);
                }
                close = true;
            }
            name if ["transfer-encoding", "upgrade", "expect", "trailer"]
                .iter()
                .any(|forbidden| name.eq_ignore_ascii_case(forbidden)) =>
            {
                return Err(Error::Framing);
            }
            _ => {}
        }
    }
    if !host || !close {
        return Err(Error::Framing);
    }
    Ok((method.to_owned(), path.to_owned(), length.ok_or(Error::Framing)?))
}
