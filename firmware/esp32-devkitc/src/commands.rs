//! AERO v2: fixed 60-byte authenticated datagram, also accepted as @hex\n on UART.
//! HMAC and parsing belong to the communications core, never the flight loop.
use flight_core::commands::Command;
use hmac::{Hmac, Mac};
use sha2::Sha256;
pub const PACKET_BYTES: usize = 60;
pub const COMMAND_EXPIRY_US: u32 = 250_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Length,
    Header,
    Session,
    Authentication,
    Range,
    Replay,
}
#[derive(Clone, Copy, Debug)]
pub struct Accepted {
    pub command: Command,
    pub received_us: u32,
    pub emergency: bool,
}
pub struct Receiver {
    key: [u8; 32],
    session: u64,
    last_sequence: Option<u32>,
    accepted: u64,
}
impl Receiver {
    pub fn next_sequence(&self) -> u32 {
        self.last_sequence.map_or(1, |v| v.wrapping_add(1))
    }
    pub fn new(key: [u8; 32], session: u64) -> Self {
        Self {
            key,
            session,
            last_sequence: None,
            accepted: 0,
        }
    }
    pub fn receive(&mut self, p: &[u8], now_us: u32) -> Result<Accepted, Error> {
        if p.len() != PACKET_BYTES {
            return Err(Error::Length);
        }
        if &p[..4] != b"AERO" || p[4] != 2 || p[5] & !3 != 0 || p[6..8] != [60, 0] || p[5] == 3 {
            return Err(Error::Header);
        }
        if u64::from_le_bytes(p[8..16].try_into().unwrap()) != self.session {
            return Err(Error::Session);
        }
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.key).unwrap();
        mac.update(&p[..28]);
        mac.verify_slice(&p[28..])
            .map_err(|_| Error::Authentication)?;
        let seq = u32::from_le_bytes(p[16..20].try_into().unwrap());
        if self
            .last_sequence
            .is_some_and(|last| seq.wrapping_sub(last) == 0 || seq.wrapping_sub(last) >= 0x80000000)
        {
            return Err(Error::Replay);
        }
        let throttle = u16::from_le_bytes([p[20], p[21]]);
        let axis = |i| i16::from_le_bytes([p[i], p[i + 1]]);
        if throttle > 1000
            || [axis(22), axis(24), axis(26)]
                .iter()
                .any(|v| !(-1000..=1000).contains(v))
        {
            return Err(Error::Range);
        }
        self.last_sequence = Some(seq);
        self.accepted += 1;
        Ok(Accepted {
            command: Command {
                sequence: self.accepted,
                throttle: throttle as f32 / 1000.,
                roll: axis(22) as f32 / 1000.,
                pitch: axis(24) as f32 / 1000.,
                yaw: axis(26) as f32 / 1000.,
                arm: p[5] & 1 != 0,
            },
            received_us: now_us,
            emergency: p[5] & 2 != 0,
        })
    }
}
pub fn encode(
    key: &[u8; 32],
    session: u64,
    sequence: u32,
    command: Command,
    emergency: bool,
) -> [u8; 60] {
    let mut p = [0; 60];
    p[..4].copy_from_slice(b"AERO");
    p[4] = 2;
    p[5] = if emergency { 2 } else { u8::from(command.arm) };
    p[6] = 60;
    p[8..16].copy_from_slice(&session.to_le_bytes());
    p[16..20].copy_from_slice(&sequence.to_le_bytes());
    p[20..22].copy_from_slice(&((command.throttle * 1000.) as u16).to_le_bytes());
    for (i, v) in [(22, command.roll), (24, command.pitch), (26, command.yaw)] {
        p[i..i + 2].copy_from_slice(&((v * 1000.) as i16).to_le_bytes());
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
    mac.update(&p[..28]);
    p[28..].copy_from_slice(&mac.finalize().into_bytes());
    p
}
pub struct UartFrame {
    data: [u8; 60],
    nibbles: usize,
    active: bool,
    overflow: bool,
}
impl Default for UartFrame {
    fn default() -> Self {
        Self {
            data: [0; 60],
            nibbles: 0,
            active: false,
            overflow: false,
        }
    }
}
impl UartFrame {
    pub fn push(&mut self, byte: u8) -> Option<[u8; 60]> {
        if byte == b'@' {
            self.active = true;
            self.nibbles = 0;
            self.overflow = false;
            return None;
        }
        if byte == b'\n' {
            let ready = self.active && !self.overflow && self.nibbles == 120;
            self.active = false;
            return ready.then_some(self.data);
        }
        if !self.active || byte == b'\r' {
            return None;
        }
        let hex = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => {
                self.overflow = true;
                return None;
            }
        };
        if self.nibbles >= 120 {
            self.overflow = true;
            return None;
        }
        let i = self.nibbles / 2;
        if self.nibbles.is_multiple_of(2) {
            self.data[i] = hex << 4;
        } else {
            self.data[i] |= hex;
        }
        self.nibbles += 1;
        None
    }
}
