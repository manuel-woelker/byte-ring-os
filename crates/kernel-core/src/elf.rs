//! Bounded static ELF64 loader policy; no casts to native ELF structs.
use crate::user_abi::{BASE, IMAGE_PAGES, PAGE, read_u32, read_u64};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Error {
    Header,
    Segment,
    Permissions,
    Entry,
    Unsupported,
}
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Permission {
    Absent,
    Read,
    Execute,
    Write,
}
#[derive(Clone, Copy)]
struct Segment {
    address: usize,
    offset: usize,
    file: usize,
    memory: usize,
    permission: Permission,
}

pub struct Image<'a> {
    bytes: &'a [u8],
    segments: [Option<Segment>; 4],
    entry: u64,
}
impl<'a> Image<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        if bytes.len() < 64
            || &bytes[..7] != b"\x7fELF\x02\x01\x01"
            || bytes[7] != 0
            || u16::from_le_bytes([bytes[16], bytes[17]]) != 2
            || u16::from_le_bytes([bytes[18], bytes[19]]) != 62
            || read_u32(bytes, 20) != 1
            || bytes[52..56] != [64, 0, 56, 0]
        {
            return Err(Error::Header);
        }
        let count = u16::from_le_bytes([bytes[56], bytes[57]]) as usize;
        let table = usize::try_from(read_u64(bytes, 32)).map_err(|_| Error::Header)?;
        if count == 0
            || count > 8
            || table < 64
            || table
                .checked_add(count * 56)
                .is_none_or(|end| end > bytes.len())
        {
            return Err(Error::Header);
        }
        let mut image = Self {
            bytes,
            segments: [None; 4],
            entry: read_u64(bytes, 24),
        };
        let mut loaded = 0;
        let mut previous_end = BASE;
        let mut executable_entry = false;
        for index in 0..count {
            let header = &bytes[table + index * 56..][..56];
            let kind = read_u32(header, 0);
            if kind == 0 {
                continue;
            }
            if kind == 0x6474e551 {
                // GNU_STACK: only a non-executable stack is accepted.
                if read_u32(header, 4) & 1 != 0 {
                    return Err(Error::Permissions);
                }
                continue;
            }
            if kind != 1 {
                return Err(Error::Unsupported);
            }
            if loaded == image.segments.len() {
                return Err(Error::Segment);
            }
            let flags = read_u32(header, 4);
            let permission = match flags {
                4 => Permission::Read,
                5 => Permission::Execute,
                6 => Permission::Write,
                _ => return Err(Error::Permissions),
            };
            let offset = read_u64(header, 8);
            let address = read_u64(header, 16);
            let file = read_u64(header, 32);
            let memory = read_u64(header, 40);
            let alignment = read_u64(header, 48);
            if memory == 0
                || file > memory
                || address < previous_end
                || address < BASE
                || !address.is_multiple_of(PAGE as u64)
                || !offset.is_multiple_of(PAGE as u64)
                || (alignment > 1
                    && (!alignment.is_power_of_two() || address % alignment != offset % alignment))
                || address
                    .checked_add(memory)
                    .is_none_or(|end| end > BASE + (IMAGE_PAGES * PAGE) as u64)
                || offset
                    .checked_add(file)
                    .is_none_or(|end| end > bytes.len() as u64)
            {
                return Err(Error::Segment);
            }
            let end = address + memory;
            previous_end = end.div_ceil(PAGE as u64) * PAGE as u64;
            executable_entry |= permission == Permission::Execute
                && (address..address + file).contains(&image.entry);
            image.segments[loaded] = Some(Segment {
                address: (address - BASE) as usize,
                offset: offset as usize,
                file: file as usize,
                memory: memory as usize,
                permission,
            });
            loaded += 1;
        }
        if !executable_entry {
            return Err(Error::Entry);
        }
        Ok(image)
    }
    pub fn entry(&self) -> u64 {
        self.entry
    }
    pub fn permission(&self, page: usize) -> Permission {
        for segment in self.segments.iter().flatten() {
            if (segment.address / PAGE..(segment.address + segment.memory).div_ceil(PAGE))
                .contains(&page)
            {
                return segment.permission;
            }
        }
        Permission::Absent
    }
    /// Initialize the complete private backing, including BSS and old contents.
    pub fn load(&self, backing: &mut [u8; IMAGE_PAGES * PAGE]) {
        backing.fill(0);
        for segment in self.segments.iter().flatten() {
            backing[segment.address..segment.address + segment.file]
                .copy_from_slice(&self.bytes[segment.offset..segment.offset + segment.file]);
        }
    }
    pub fn executable(&self, address: u64) -> bool {
        self.segments.iter().flatten().any(|s| {
            s.permission == Permission::Execute
                && (BASE + s.address as u64..BASE + (s.address + s.file) as u64).contains(&address)
        })
    }
}

#[cfg(test)]
#[path = "elf_tests.rs"]
mod tests;
