//! Order keys compatible with the `fractional-indexing` npm package used by the board UI
//! (base-62 digits, variable-length integer part). The core only needs "append after".
const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

fn int_len(head: u8) -> usize {
    match head {
        b'a'..=b'z' => (head - b'a') as usize + 2,
        b'A'..=b'Z' => (b'Z' - head) as usize + 2,
        _ => 2,
    }
}

fn increment_integer(x: &str) -> Option<String> {
    let bytes = x.as_bytes();
    let head = bytes[0];
    let mut digs: Vec<u8> = bytes[1..].to_vec();
    let mut carry = true;
    for i in (0..digs.len()).rev() {
        let d = DIGITS.iter().position(|&c| c == digs[i]).unwrap_or(0) + 1;
        if d == DIGITS.len() {
            digs[i] = b'0';
        } else {
            digs[i] = DIGITS[d];
            carry = false;
            break;
        }
    }
    if !carry {
        let mut out = vec![head];
        out.extend(digs);
        return Some(String::from_utf8(out).unwrap());
    }
    match head {
        b'Z' => Some("a0".into()),
        b'z' => None,
        _ => {
            let h = head + 1;
            if h > b'a' { digs.push(b'0') } else { digs.pop(); }
            let mut out = vec![h];
            out.extend(digs);
            Some(String::from_utf8(out).unwrap())
        }
    }
}

/// A key that sorts after `prev` (or the first key when there is none).
pub fn key_after(prev: Option<&str>) -> String {
    match prev {
        None => "a0".into(),
        Some(k) if k.is_empty() => "a0".into(),
        Some(k) => {
            let n = int_len(k.as_bytes()[0]).min(k.len());
            increment_integer(&k[..n]).unwrap_or_else(|| format!("{k}V"))
        }
    }
}

/// A key that sorts between `prev` and `next` (plain string order, as columns are sorted), never ending in "0".
pub fn key_between(prev: Option<&str>, next: Option<&str>) -> String {
    let Some(next) = next else { return key_after(prev) };
    let prev = prev.unwrap_or("");
    match next.strip_prefix(prev) {
        // `next` continues `prev`: a digit below where it goes on.
        Some(rest) if !rest.is_empty() => {
            let at = DIGITS.iter().position(|&c| c == rest.as_bytes()[0]).unwrap_or(0);
            match at {
                0 => key_between(Some(&format!("{prev}0")), Some(next)),
                1 => format!("{prev}0V"),
                _ => format!("{prev}{}", DIGITS[at / 2] as char),
            }
        }
        // They differ before `prev` ends: anything that continues `prev` sorts before `next`.
        _ => format!("{prev}V"),
    }
}
