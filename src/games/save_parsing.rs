use std::{io::Cursor, str::from_utf8};

use anyhow::{anyhow, Result};
use byteorder::{ByteOrder, LittleEndian, ReadBytesExt};
use chrono::naive::NaiveTime;

use crate::helpers::BoxedError;

const Z3_SM_SRAM_CHECKSUM: u16 = 0x55AA;
const Z3R_ROM_NAMES: [&'static str; 2] = ["VT", "ER"];

pub struct Z3rSram([u8; 32768]);
pub struct SMZ3Sram([u8; 32768]);
pub struct SMTotalSram([u8; 16384]);
pub struct SMVARIASram([u8; 8192]);

pub trait SaveParser {
    fn game_finished(&self) -> bool;

    fn get_igt(&self) -> Result<NaiveTime, BoxedError>;

    fn get_collection_rate(&self) -> Option<u64>;
}

fn get_stat(
    cur: &mut Cursor<&[u8]>,
    offset: u64,
    bits: u32,
    shift: u32,
) -> Result<u64, BoxedError> {
    cur.set_position(offset);
    let bytes: f32 = ((bits as f32 + shift as f32) / 8f32).ceil();
    let mut value = match bytes as u8 {
        1 => cur.read_u8().unwrap() as u32,
        2 => cur.read_u16::<LittleEndian>().unwrap() as u32,
        _ => return Err(anyhow!("Tried reading more than two bytes at {}", offset).into()),
    };
    value >>= shift;
    value &= bitmask(bits);

    Ok(value as u64)
}

fn new_32_le_time(cur: &mut Cursor<&[u8]>, pos: u64) -> String {
    let value: u32 = {
        cur.set_position(pos);
        cur.read_u32::<LittleEndian>().unwrap()
    };
    let hours: u32 = value / 216000u32;
    let mut rem = value % 216000u32;
    let minutes: u32 = rem / 3600u32;
    rem %= 3600u32;
    let seconds: u32 = rem / 60u32;

    let time = format!("{:0>2}:{:0>2}:{:0>2}", hours, minutes, seconds);

    time
}

fn new_smz3_time(cur: &mut Cursor<&[u8]>) -> String {
    let z3_value: u32 = {
        cur.set_position(0x43E);
        cur.read_u32::<LittleEndian>().unwrap()
    };
    let sm_value: u32 = {
        cur.set_position(0x3A00);
        cur.read_u32::<LittleEndian>().unwrap()
    };
    let value = z3_value + sm_value;
    let hours: u32 = value / (216000u32);
    let mut rem = value % 216000u32;
    let minutes: u32 = rem / 3600u32;
    rem %= 3600u32;
    let seconds: u32 = rem / 60u32;

    let time = format!("{:0>2}:{:0>2}:{:0>2}", hours, minutes, seconds);

    time
}

#[inline]
pub fn bitmask(bits: u32) -> u32 {
    (1u32 << bits) - 1u32
}

fn get_set_bits<T: Into<u32>>(n: T) -> u8 {
    let mut value = n.into();
    if value == 0 {
        return 0;
    }
    let mut count: u8 = 0;
    while value > 0 {
        value &= value - 1;
        count += 1;
    }

    // this is fine
    count as u8
}

// https://github.com/cassidoxa/z3r-sramr/

impl Z3rSram {
    pub fn new_from_slice(s: &Vec<u8>) -> Result<Z3rSram, BoxedError> {
        if s.len() != 32768 {
            return Err(anyhow!("Incorrect file size for ALTTPR SRAM").into());
        }
        // i can't figure out how to use a ref to the actual attachment so let's
        // just copy into a buffer here
        let mut buf = [0; 32768];
        buf.copy_from_slice(s);

        let mut cur = Cursor::new(buf);
        let checksum_validity: u16 = LittleEndian::read_u16(&buf[0x3E1..0x3E3]);
        if checksum_validity != Z3_SM_SRAM_CHECKSUM || buf[0x4F0] != 0xFF {
            return Err(anyhow!("ALTTPR SRAM Validation Error: Invalid file").into());
        }
        // Check the first two characters of the rom name for VT or ER
        let rom_name = &buf[0x2000..0x2002];
        if Z3R_ROM_NAMES
            .iter()
            .any(|&x| x == from_utf8(&rom_name).unwrap())
            == false
        {
            return Err(anyhow!("ALTTPR SRAM Validation Error: Invalid ROM name").into());
        }
        // Now we check the SRAM's own "inverse" checksum
        let mut checksum = 0u16;
        cur.set_position(0x00);
        while cur.position() < 0x4FE {
            let bytes = cur.read_u16::<LittleEndian>()?;
            checksum = checksum.overflowing_add(bytes).0;
        }
        let expected_inv_checksum = 0x5A5Au16.overflowing_sub(checksum).0;
        let inv_checksum: u16 = LittleEndian::read_u16(&buf[0x4FE..0x500]);
        if inv_checksum != expected_inv_checksum {
            return Err(anyhow!("ALTTPR SRAM Validation Error: Invalid checksum").into());
        }
        Ok(Z3rSram(buf))
    }
}

impl SaveParser for Z3rSram {
    fn game_finished(&self) -> bool {
        let slice = &self.0[..];
        let mut cur = Cursor::new(slice);
        let finished = get_stat(&mut cur, 0x443, 8, 0).unwrap();
        match finished {
            1u64 => true,
            0u64 => false,
            _ => false,
        }
    }

    fn get_igt(&self) -> Result<NaiveTime, BoxedError> {
        // just remembered that every array size is a distinct type
        // .as_slice() exists but not stable yet
        let slice = &self.0[..];
        let mut cur = Cursor::new(slice);
        let igt = new_32_le_time(&mut cur, 0x43E);
        let time = NaiveTime::parse_from_str(&igt, "%H:%M:%S")?;

        Ok(time)
    }

    fn get_collection_rate(&self) -> Option<u64> {
        let slice = &self.0[..];
        let mut cur = Cursor::new(slice);
        let collection = get_stat(&mut cur, 0x423, 8, 0).ok();

        collection
    }
}

impl SMZ3Sram {
    pub fn new_from_slice(s: &Vec<u8>) -> Result<SMZ3Sram, BoxedError> {
        if s.len() != 32768 {
            return Err(anyhow!("Incorrect file size for SMZ3 SRAM").into());
        }
        let mut buf = [0; 32768];
        buf.copy_from_slice(s);

        let mut cur = Cursor::new(buf);
        let checksum_validity: u16 = LittleEndian::read_u16(&buf[0x3E1..0x3E3]);
        if checksum_validity != Z3_SM_SRAM_CHECKSUM || buf[0x4F0] != 0xFF {
            return Err(anyhow!("ALTTPR SRAM Validation Error: Invalid file").into());
        }
        let mut checksum = 0u16;
        cur.set_position(0x00);
        while cur.position() < 0x4FE {
            let bytes = cur.read_u16::<LittleEndian>()?;
            checksum = checksum.overflowing_add(bytes).0;
        }

        Ok(SMZ3Sram(buf))
    }
}

impl SaveParser for SMZ3Sram {
    fn game_finished(&self) -> bool {
        let slice = &self.0[..];
        let mut cur = Cursor::new(slice);
        let z3_finished = get_stat(&mut cur, 0x3402, 8, 0).unwrap();
        let sm_finished = get_stat(&mut cur, 0x3506, 8, 0).unwrap();

        z3_finished == 1u64 && sm_finished == 1u64
    }

    fn get_igt(&self) -> Result<NaiveTime, BoxedError> {
        let slice = &self.0[..];
        let mut cur = Cursor::new(slice);
        let igt = new_smz3_time(&mut cur);
        let time = NaiveTime::parse_from_str(&igt, "%H:%M:%S")?;

        Ok(time)
    }

    fn get_collection_rate(&self) -> Option<u64> {
        let slice = &self.0[..];
        let mut cur = Cursor::new(slice);
        let z3_collection = get_stat(&mut cur, 0x423, 8, 0).unwrap();
        let sm_collection = get_stat(&mut cur, 0x3A3A, 8, 0).unwrap();

        let collection = z3_collection + sm_collection;

        Some(collection)
    }
}

impl SMTotalSram {
    pub fn new_from_slice(s: &Vec<u8>) -> Result<SMTotalSram, BoxedError> {
        if s.len() != 16384 {
            return Err(anyhow!("Incorrect file size for SM Total SRAM").into());
        }

        let mut buf = [0; 16384];
        buf.copy_from_slice(s);
        let mut cur = Cursor::new(buf);

        let expected_checksum: u16 = LittleEndian::read_u16(&buf[0x00..0x02]);
        let mut checksum = 0u16;
        cur.set_position(0x10);
        while cur.position() < 0x65C {
            let bytes = cur.read_u16::<LittleEndian>().unwrap();
            checksum = checksum.overflowing_add(bytes).0;
        }
        match expected_checksum == checksum {
            true => Ok(SMTotalSram(buf)),
            false => Err(anyhow!("SM SRAM has invalid checksum").into()),
        }
    }
}

impl SaveParser for SMTotalSram {
    fn game_finished(&self) -> bool {
        let str_slice = &self.0[0x1FE0..0x1FEC];
        match from_utf8(&str_slice) {
            // i think this may be a weird side effect but it seems to work
            // for now
            Ok(s) if s == "supermetroid" => true,
            _ => false,
        }
    }

    fn get_igt(&self) -> Result<NaiveTime, BoxedError> {
        let slice = &self.0[..];
        let mut cur = Cursor::new(slice);
        let igt = new_32_le_time(&mut cur, 0x1400);
        let time = NaiveTime::parse_from_str(&igt, "%H:%M:%S")?;

        Ok(time)
    }

    fn get_collection_rate(&self) -> Option<u64> {
        let mut collection: u8 = 0;

        // missiles
        collection += (&self.0[0x36]) / 5;
        // super missiles
        collection += (&self.0[0x3A]) / 5;
        // power bombs
        collection += (&self.0[0x3E]) / 5;
        // e-tanks
        collection += ((&self.0[0x32]) + 1) / 100;
        // reserve
        collection += (&self.0[0x42]) / 100;
        // items
        let items: u16 = LittleEndian::read_u16(&self.0[0x12..0x15]);
        collection += get_set_bits(items);
        // beams
        let beams: u16 = LittleEndian::read_u16(&self.0[0x16..0x18]);
        collection += get_set_bits(beams);

        Some(collection as u64)
    }
}

impl SMVARIASram {
    pub fn new_from_slice(s: &Vec<u8>) -> Result<SMVARIASram, BoxedError> {
        if s.len() != 8192 {
            return Err(anyhow!("Incorrect file size for SM VARIA SRAM").into());
        }

        let mut buf = [0; 8192];
        buf.copy_from_slice(s);
        let mut cur = Cursor::new(buf);

        let expected_checksum: u16 = LittleEndian::read_u16(&buf[0x00..0x02]);
        let mut checksum = 0u16;
        cur.set_position(0x10);
        while cur.position() < 0x65C {
            let bytes = cur.read_u16::<LittleEndian>().unwrap();
            checksum = checksum.overflowing_add(bytes).0;
        }
        match expected_checksum == checksum {
            true => Ok(SMVARIASram(buf)),
            false => Err(anyhow!("SM SRAM has invalid checksum").into()),
        }
    }
}

impl SaveParser for SMVARIASram {
    fn game_finished(&self) -> bool {
        let str_slice = &self.0[0x1FE0..0x1FEC];
        match from_utf8(&str_slice) {
            // i think this may be a weird side effect but it seems to work
            // for now
            Ok(s) if s == "supermetroid" => true,
            _ => false,
        }
    }

    fn get_igt(&self) -> Result<NaiveTime, BoxedError> {
        let slice = &self.0[..];
        let mut cur = Cursor::new(slice);
        let igt = new_32_le_time(&mut cur, 0x1400);
        let time = NaiveTime::parse_from_str(&igt, "%H:%M:%S")?;

        Ok(time)
    }

    fn get_collection_rate(&self) -> Option<u64> {
        let mut collection: u8 = 0;

        // missiles
        collection += (&self.0[0x36]) / 5;
        // super missiles
        collection += (&self.0[0x3A]) / 5;
        // power bombs
        collection += (&self.0[0x3E]) / 5;
        // e-tanks
        collection += ((&self.0[0x32]) + 1) / 100;
        // reserve
        collection += (&self.0[0x42]) / 100;
        // items
        let items: u16 = LittleEndian::read_u16(&self.0[0x12..0x15]);
        collection += get_set_bits(items);
        // beams
        let beams: u16 = LittleEndian::read_u16(&self.0[0x16..0x18]);
        collection += get_set_bits(beams);

        Some(collection as u64)
    }
}
