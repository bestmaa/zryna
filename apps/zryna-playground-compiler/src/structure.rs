pub(super) fn validate(bytes: &[u8]) -> Result<(), &'static str> {
    let mut stack = Vec::<usize>::new();
    let mut quoted = false;
    let mut escaped = false;
    let mut work = 0_usize;
    for byte in bytes {
        if quoted {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                quoted = false;
            }
            continue;
        }
        match byte {
            b'"' => quoted = true,
            b'{' | b'[' => {
                stack.push(1);
                if stack.len() > 8 {
                    return Err("PLAYGROUND-REQUEST-STRUCTURE");
                }
            }
            b'}' | b']' => {
                stack.pop();
            }
            b',' => {
                if let Some(count) = stack.last_mut() {
                    *count += 1;
                    if *count > 512 {
                        return Err("PLAYGROUND-REQUEST-STRUCTURE");
                    }
                }
            }
            _ => {}
        }
        if !matches!(byte, b' ' | b'\t' | b'\r' | b'\n') {
            work += 1;
            if work > 16_384 {
                return Err("PLAYGROUND-REQUEST-STRUCTURE");
            }
        }
    }
    Ok(())
}
