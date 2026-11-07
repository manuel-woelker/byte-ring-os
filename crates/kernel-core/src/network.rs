//! Bounded Ethernet/ARP/IPv4/ICMP responder for the initial isolated /24 subnet.

pub const MTU: usize = 1500;
pub const MAX_FRAME: usize = 14 + MTU;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    Truncated,
    Oversize,
    Destination,
    Unsupported,
    Checksum,
    Source,
    OutputFull,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Echo {
    pub source: [u8; 4],
    pub identifier: u16,
    pub sequence: u16,
    pub payload_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reply {
    pub length: usize,
    pub echo: Option<Echo>,
}

pub struct Network {
    pub ip: [u8; 4],
    pub mac: [u8; 6],
}

/// Internet checksum, including an odd final byte in the high octet.
pub fn checksum(bytes: &[u8]) -> u16 {
    let mut sum = 0u32;
    for pair in bytes.chunks(2) {
        sum += ((pair[0] as u32) << 8) | pair.get(1).copied().unwrap_or(0) as u32;
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !((sum & 0xffff) + (sum >> 16)) as u16
}

fn word(bytes: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([bytes[offset], bytes[offset + 1]])
}
fn put(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

impl Network {
    fn valid_source(&self, ip: &[u8]) -> bool {
        ip[..3] == self.ip[..3] && ip[3] != 0 && ip[3] != 255 && ip != self.ip
    }

    /// Return a reply description or an ordinary drop; never allocate or retain
    /// input memory. Only a valid reply's `length` bytes may be transmitted.
    pub fn respond(&self, frame: &[u8], output: &mut [u8]) -> Result<Reply, DropReason> {
        if frame.len() < 14 {
            return Err(DropReason::Truncated);
        }
        if frame.len() > MAX_FRAME {
            return Err(DropReason::Oversize);
        }
        if frame[..6] != self.mac && frame[..6] != [255; 6] {
            return Err(DropReason::Destination);
        }
        if frame[6] & 1 != 0 || frame[6..12] == [0; 6] || frame[6..12] == self.mac {
            return Err(DropReason::Source);
        }
        match word(frame, 12) {
            0x0806 => self.arp(frame, output),
            0x0800 => self.ipv4(frame, output),
            _ => Err(DropReason::Unsupported),
        }
    }

    fn arp(&self, frame: &[u8], output: &mut [u8]) -> Result<Reply, DropReason> {
        if frame.len() < 42 {
            return Err(DropReason::Truncated);
        }
        if frame[14..22] != [0, 1, 8, 0, 6, 4, 0, 1] {
            return Err(DropReason::Unsupported);
        }
        if frame[38..42] != self.ip {
            return Err(DropReason::Destination);
        }
        if frame[22..28] != frame[6..12] || !self.valid_source(&frame[28..32]) {
            return Err(DropReason::Source);
        }
        if output.len() < 60 {
            return Err(DropReason::OutputFull);
        }
        output[..60].fill(0);
        output[..6].copy_from_slice(&frame[6..12]);
        output[6..12].copy_from_slice(&self.mac);
        put(output, 12, 0x0806);
        output[14..22].copy_from_slice(&[0, 1, 8, 0, 6, 4, 0, 2]);
        output[22..28].copy_from_slice(&self.mac);
        output[28..32].copy_from_slice(&self.ip);
        output[32..38].copy_from_slice(&frame[22..28]);
        output[38..42].copy_from_slice(&frame[28..32]);
        Ok(Reply {
            length: 60,
            echo: None,
        })
    }

    fn ipv4(&self, frame: &[u8], output: &mut [u8]) -> Result<Reply, DropReason> {
        if frame.len() < 34 {
            return Err(DropReason::Truncated);
        }
        if frame[..6] != self.mac || frame[30..34] != self.ip {
            return Err(DropReason::Destination);
        }
        if frame[14] != 0x45 || frame[23] != 1 || frame[22] == 0 || word(frame, 20) & 0xbfff != 0 {
            return Err(DropReason::Unsupported);
        }
        let total = word(frame, 16) as usize;
        if total < 28 || frame.len() < 14 + total {
            return Err(DropReason::Truncated);
        }
        if total > MTU {
            return Err(DropReason::Oversize);
        }
        if !self.valid_source(&frame[26..30]) {
            return Err(DropReason::Source);
        }
        if checksum(&frame[14..34]) != 0 || checksum(&frame[34..14 + total]) != 0 {
            return Err(DropReason::Checksum);
        }
        if frame[34] != 8 || frame[35] != 0 {
            return Err(DropReason::Unsupported);
        }
        let length = (14 + total).max(60);
        if output.len() < length {
            return Err(DropReason::OutputFull);
        }
        output[..length].fill(0);
        output[..14 + total].copy_from_slice(&frame[..14 + total]);
        output[..6].copy_from_slice(&frame[6..12]);
        output[6..12].copy_from_slice(&self.mac);
        output[26..30].copy_from_slice(&self.ip);
        output[30..34].copy_from_slice(&frame[26..30]);
        output[22] = 64;
        put(output, 24, 0);
        let ip_sum = checksum(&output[14..34]);
        put(output, 24, ip_sum);
        output[34] = 0;
        put(output, 36, 0);
        let icmp_sum = checksum(&output[34..14 + total]);
        put(output, 36, icmp_sum);
        Ok(Reply {
            length,
            echo: Some(Echo {
                source: frame[26..30]
                    .try_into()
                    .map_err(|_| DropReason::Truncated)?,
                identifier: word(frame, 38),
                sequence: word(frame, 40),
                payload_bytes: total - 28,
            }),
        })
    }
}

#[cfg(test)]
#[path = "network_tests.rs"]
mod tests;
