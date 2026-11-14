//! Single communications writer / single flight reader. Every field is atomic:
//! no shared mutable data or cross-core critical section on the control path.
use crate::commands::Accepted;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use flight_core::commands::Command;
pub struct Mailbox {
    epoch: AtomicU32,
    words: [AtomicU32; 9],
    emergency: AtomicBool,
}
impl Default for Mailbox {
    fn default() -> Self {
        Self::new()
    }
}
impl Mailbox {
    pub const fn new() -> Self {
        Self {
            epoch: AtomicU32::new(0),
            words: [const { AtomicU32::new(0) }; 9],
            emergency: AtomicBool::new(false),
        }
    }
    pub fn take_emergency(&self) -> bool {
        self.emergency.swap(false, Ordering::SeqCst)
    }
    pub fn publish(&self, p: Accepted) {
        if p.emergency {
            self.emergency.store(true, Ordering::SeqCst);
        }
        self.epoch.fetch_add(1, Ordering::SeqCst);
        let c = p.command;
        let words = [
            c.sequence as u32,
            (c.sequence >> 32) as u32,
            c.throttle.to_bits(),
            c.roll.to_bits(),
            c.pitch.to_bits(),
            c.yaw.to_bits(),
            u32::from(c.arm),
            p.received_us,
            u32::from(p.emergency),
        ];
        for (target, value) in self.words.iter().zip(words) {
            target.store(value, Ordering::SeqCst);
        }
        self.epoch.fetch_add(1, Ordering::SeqCst);
    }
    pub fn read_new(&self, last: &mut u32) -> Option<Accepted> {
        // Bound contention: a network update can cost at most two attempts.
        for _ in 0..2 {
            let start = self.epoch.load(Ordering::SeqCst);
            if start == 0 || start == *last || !start.is_multiple_of(2) {
                return None;
            }
            let w = self.words.each_ref().map(|w| w.load(Ordering::SeqCst));
            if self.epoch.load(Ordering::SeqCst) != start {
                continue;
            }
            *last = start;
            return Some(Accepted {
                command: Command {
                    sequence: w[0] as u64 | ((w[1] as u64) << 32),
                    throttle: f32::from_bits(w[2]),
                    roll: f32::from_bits(w[3]),
                    pitch: f32::from_bits(w[4]),
                    yaw: f32::from_bits(w[5]),
                    arm: w[6] != 0,
                },
                received_us: w[7],
                emergency: w[8] != 0,
            });
        }
        None
    }
}
