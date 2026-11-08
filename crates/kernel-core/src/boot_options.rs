//! Bounded, pointer-free parsing of the copied Multiboot command line.
pub const MAX_COMMAND_LINE: usize = 256;
pub const LAST_SCENARIO: u64 = 16;
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Error {
    TooLong,
    Encoding,
    Scenario,
}

pub fn scenario(command: &[u8]) -> Result<Option<u64>, Error> {
    if command.len() >= MAX_COMMAND_LINE {
        return Err(Error::TooLong);
    }
    let command = core::str::from_utf8(command).map_err(|_| Error::Encoding)?;
    let mut selected = None;
    for word in command.split_ascii_whitespace() {
        let Some(value) = word.strip_prefix("user-scenario=") else {
            continue;
        };
        if selected.is_some() || value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Error::Scenario);
        }
        let value = value.parse::<u64>().map_err(|_| Error::Scenario)?;
        if value > LAST_SCENARIO {
            return Err(Error::Scenario);
        }
        selected = Some(value);
    }
    Ok(selected)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_and_explicit_selection_accept_loader_prefix() {
        assert_eq!(scenario(b"kernel.bin"), Ok(None));
        assert_eq!(scenario(b"kernel.bin user-scenario=6"), Ok(Some(6)));
        assert_eq!(scenario(b"user-scenario=0 other=1"), Ok(Some(0)));
        assert_eq!(scenario(b"user-scenario=16"), Ok(Some(16)));
    }
    #[test]
    fn malformed_ambiguous_and_oversized_input_is_rejected() {
        for bytes in [
            b"user-scenario=".as_slice(),
            b"user-scenario=-1",
            b"user-scenario=17",
            b"user-scenario=18446744073709551616",
            b"user-scenario=0 user-scenario=1",
            b"user-scenario=+1",
        ] {
            assert_eq!(scenario(bytes), Err(Error::Scenario));
        }
        assert_eq!(scenario(&[b'a'; MAX_COMMAND_LINE]), Err(Error::TooLong));
        assert_eq!(scenario(b"\xff"), Err(Error::Encoding));
    }
}
