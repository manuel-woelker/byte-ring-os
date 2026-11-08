//! Bounded, pointer-free parsing of the copied Multiboot command line.
pub const MAX_COMMAND_LINE: usize = 256;
pub const LAST_SCENARIO: u64 = 18;
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Error {
    TooLong,
    Encoding,
    Scenario,
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct Options {
    pub scenario: Option<u64>,
    pub peer: Option<u64>,
}
pub fn scenario(command: &[u8]) -> Result<Option<u64>, Error> {
    options(command).map(|o| o.scenario)
}
pub fn options(command: &[u8]) -> Result<Options, Error> {
    if command.len() >= MAX_COMMAND_LINE {
        return Err(Error::TooLong);
    }
    let command = core::str::from_utf8(command).map_err(|_| Error::Encoding)?;
    let mut result = Options::default();
    for word in command.split_ascii_whitespace() {
        let (selected, value) = if let Some(value) = word.strip_prefix("user-scenario=") {
            (&mut result.scenario, value)
        } else if let Some(value) = word.strip_prefix("user-peer=") {
            (&mut result.peer, value)
        } else {
            continue;
        };
        if selected.is_some() || value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Error::Scenario);
        }
        let value = value.parse::<u64>().map_err(|_| Error::Scenario)?;
        if value > LAST_SCENARIO {
            return Err(Error::Scenario);
        }
        *selected = Some(value);
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_and_explicit_selection_accept_loader_prefix() {
        assert_eq!(scenario(b"kernel.bin"), Ok(None));
        assert_eq!(scenario(b"kernel.bin user-scenario=6"), Ok(Some(6)));
        assert_eq!(scenario(b"user-scenario=0 other=1"), Ok(Some(0)));
        assert_eq!(scenario(b"user-scenario=18"), Ok(Some(18)));
        assert_eq!(
            options(b"user-scenario=17 user-peer=18"),
            Ok(Options {
                scenario: Some(17),
                peer: Some(18)
            })
        );
        assert_eq!(
            options(b"user-peer=6"),
            Ok(Options {
                scenario: None,
                peer: Some(6)
            })
        );
    }
    #[test]
    fn malformed_ambiguous_and_oversized_input_is_rejected() {
        for bytes in [
            b"user-scenario=".as_slice(),
            b"user-scenario=-1",
            b"user-scenario=19",
            b"user-scenario=18446744073709551616",
            b"user-scenario=0 user-scenario=1",
            b"user-scenario=+1",
            b"user-peer=19",
            b"user-peer=0 user-peer=1",
        ] {
            assert_eq!(scenario(bytes), Err(Error::Scenario));
        }
        assert_eq!(scenario(&[b'a'; MAX_COMMAND_LINE]), Err(Error::TooLong));
        assert_eq!(scenario(b"\xff"), Err(Error::Encoding));
    }
}
