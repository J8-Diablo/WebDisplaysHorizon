//! Messages larger than Horizon lets through, cut in fragments and put back together.
//!
//! Horizon's mod messages stop at 30,000 bytes each and 32 a tick for each player and
//! direction (docs/native-mods.md, "Mod messages"). A screen list or a friend list can be
//! more than that, so every message of the mod travels as one or more fragments:
//!
//! ```text
//! 0  message id (u32)   4  index (u16)   6  count (u16)   8  payload
//! ```
//!
//! What arrives is from another machine and is trusted with nothing: sizes, counts and
//! how much may wait half-assembled are all bounded here, so a player cannot make the
//! server (or a server a client) hold memory for them.
use std::collections::VecDeque;

/// Bytes of a fragment's header.
pub const HEADER: usize = 8;
/// The most a fragment weighs, header included; under Horizon's 30,000 with room to spare.
pub const MAX_FRAGMENT: usize = 28_000;
/// The most payload a fragment carries.
pub const MAX_PAYLOAD: usize = MAX_FRAGMENT - HEADER;
/// The most fragments one message is cut in: 64 of them is just under 1.8 MB.
pub const MAX_FRAGMENTS: usize = 64;
/// Fragments sent to (or from) one player in a tick, a quarter of what Horizon allows:
/// the rest is left to the mod's small messages.
pub const PER_TICK: usize = 8;
/// Messages one sender may have half-arrived at once.
const MAX_PARTIAL: usize = 4;
/// Ticks a half-arrived message waits for its next fragment before it is dropped.
const PATIENCE: u64 = 200;

/// Why a message was not sent or a fragment not taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// More than `MAX_FRAGMENTS` fragments' worth.
    TooLarge,
    /// A fragment shorter than its header, heavier than `MAX_FRAGMENT`, or with an index
    /// or count that cannot be.
    Malformed,
    /// It disagrees with the fragments of the same message that came before.
    Inconsistent,
    /// The sender already has `MAX_PARTIAL` messages half-arrived.
    TooMany,
}

/// Cuts `message` in fragments ready to send, under message id `id`.
pub fn split(id: u32, message: &[u8]) -> Result<Vec<Vec<u8>>, Refused> {
    let count = message.len().div_ceil(MAX_PAYLOAD).max(1);
    if count > MAX_FRAGMENTS {
        return Err(Refused::TooLarge);
    }
    let mut payloads = message.chunks(MAX_PAYLOAD);
    Ok((0..count)
        .map(|index| {
            let payload = payloads.next().unwrap_or_default();
            let mut fragment = Vec::with_capacity(HEADER + payload.len());
            fragment.extend(id.to_le_bytes());
            fragment.extend((index as u16).to_le_bytes());
            fragment.extend((count as u16).to_le_bytes());
            fragment.extend(payload);
            fragment
        })
        .collect())
}

/// A message some of whose fragments have arrived.
struct Partial {
    id: u32,
    parts: Vec<Option<Vec<u8>>>,
    /// The tick its last fragment came.
    seen: u64,
}

/// Puts one sender's fragments back together. Keep one for each player on the server, and
/// one on the client for the server.
#[derive(Default)]
pub struct Assembler {
    partial: Vec<Partial>,
}

impl Assembler {
    /// Takes a fragment that arrived at tick `now`; gives the whole message once its last
    /// fragment is in, whatever order they came in.
    pub fn accept(&mut self, fragment: &[u8], now: u64) -> Result<Option<Vec<u8>>, Refused> {
        self.partial.retain(|partial| now.saturating_sub(partial.seen) <= PATIENCE);
        if fragment.len() < HEADER || fragment.len() > MAX_FRAGMENT {
            return Err(Refused::Malformed);
        }
        let id = u32::from_le_bytes(fragment[0..4].try_into().unwrap());
        let index = u16::from_le_bytes(fragment[4..6].try_into().unwrap()) as usize;
        let count = u16::from_le_bytes(fragment[6..8].try_into().unwrap()) as usize;
        let payload = &fragment[HEADER..];
        if count == 0 || count > MAX_FRAGMENTS || index >= count {
            return Err(Refused::Malformed);
        }
        if count == 1 {
            return Ok(Some(payload.to_vec()));
        }
        let at = match self.partial.iter().position(|partial| partial.id == id) {
            Some(at) => at,
            None if self.partial.len() >= MAX_PARTIAL => return Err(Refused::TooMany),
            None => {
                self.partial.push(Partial { id, parts: vec![None; count], seen: now });
                self.partial.len() - 1
            }
        };
        let partial = &mut self.partial[at];
        if partial.parts.len() != count || partial.parts[index].is_some() {
            // A second count for one message, or the same fragment twice: drop the lot.
            self.partial.swap_remove(at);
            return Err(Refused::Inconsistent);
        }
        partial.parts[index] = Some(payload.to_vec());
        partial.seen = now;
        if partial.parts.iter().any(Option::is_none) {
            return Ok(None);
        }
        let whole = self.partial.swap_remove(at).parts.into_iter().flatten().flatten().collect();
        Ok(Some(whole))
    }

    /// Messages still waiting for fragments.
    pub fn waiting(&self) -> usize {
        self.partial.len()
    }
}

/// What is waiting to be sent to one player (or to the server), let out a few fragments a
/// tick so that a large list never crowds out the tick's other messages.
#[derive(Default)]
pub struct Outbox {
    queue: VecDeque<Vec<u8>>,
    next_id: u32,
}

impl Outbox {
    /// Queues a message.
    pub fn push(&mut self, message: &[u8]) -> Result<(), Refused> {
        let fragments = split(self.next_id, message)?;
        self.next_id = self.next_id.wrapping_add(1);
        self.queue.extend(fragments);
        Ok(())
    }

    /// The fragments to send this tick, `PER_TICK` at most, in the order they were queued.
    pub fn tick(&mut self) -> impl Iterator<Item = Vec<u8>> + '_ {
        let count = self.queue.len().min(PER_TICK);
        self.queue.drain(..count)
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    fn assemble(fragments: &[Vec<u8>]) -> Option<Vec<u8>> {
        let mut assembler = Assembler::default();
        let mut whole = None;
        for fragment in fragments {
            assert!(whole.is_none(), "the message was complete before its last fragment");
            whole = assembler.accept(fragment, 0).unwrap();
        }
        assert_eq!(assembler.waiting(), 0);
        whole
    }

    #[test]
    fn a_small_message_is_one_fragment_and_an_empty_one_too() {
        for len in [0, 1, MAX_PAYLOAD] {
            let fragments = split(7, &message(len)).unwrap();
            assert_eq!(fragments.len(), 1, "{len} bytes");
            assert!(fragments[0].len() <= MAX_FRAGMENT);
            assert_eq!(assemble(&fragments), Some(message(len)));
        }
    }

    #[test]
    fn a_large_message_is_cut_and_put_back_in_any_order() {
        let original = message(MAX_PAYLOAD * 3 + 5);
        let mut fragments = split(1, &original).unwrap();
        assert_eq!(fragments.len(), 4);
        assert!(fragments.iter().all(|fragment| fragment.len() <= MAX_FRAGMENT));
        assert_eq!(assemble(&fragments), Some(original.clone()));
        fragments.reverse();
        assert_eq!(assemble(&fragments), Some(original));
    }

    #[test]
    fn a_message_past_the_limit_is_refused_before_it_is_sent() {
        assert!(split(0, &message(MAX_PAYLOAD * MAX_FRAGMENTS)).is_ok());
        assert_eq!(split(0, &message(MAX_PAYLOAD * MAX_FRAGMENTS + 1)), Err(Refused::TooLarge));
    }

    #[test]
    fn two_messages_interleaved_do_not_mix() {
        let (first, second) = (message(MAX_PAYLOAD + 10), vec![9u8; MAX_PAYLOAD * 2]);
        let (a, b) = (split(1, &first).unwrap(), split(2, &second).unwrap());
        let mut assembler = Assembler::default();
        assert_eq!(assembler.accept(&a[0], 0), Ok(None));
        assert_eq!(assembler.accept(&b[1], 0), Ok(None));
        assert_eq!(assembler.accept(&a[1], 0), Ok(Some(first)));
        assert_eq!(assembler.accept(&b[0], 0), Ok(Some(second)));
    }

    #[test]
    fn nonsense_from_the_other_side_is_refused() {
        let mut assembler = Assembler::default();
        let fragment = |id: u32, index: u16, count: u16| {
            let mut bytes = id.to_le_bytes().to_vec();
            bytes.extend(index.to_le_bytes());
            bytes.extend(count.to_le_bytes());
            bytes.push(1);
            bytes
        };
        assert_eq!(assembler.accept(&[1, 2, 3], 0), Err(Refused::Malformed), "shorter than a header");
        assert_eq!(assembler.accept(&vec![0; MAX_FRAGMENT + 1], 0), Err(Refused::Malformed), "too heavy");
        assert_eq!(assembler.accept(&fragment(1, 0, 0), 0), Err(Refused::Malformed), "no fragments");
        assert_eq!(assembler.accept(&fragment(1, 3, 3), 0), Err(Refused::Malformed), "index past the count");
        assert_eq!(assembler.accept(&fragment(1, 0, MAX_FRAGMENTS as u16 + 1), 0), Err(Refused::Malformed));
        assert_eq!(assembler.waiting(), 0, "nothing of it was kept");
    }

    #[test]
    fn a_repeated_fragment_or_a_changed_count_drops_the_message() {
        let fragments = split(5, &message(MAX_PAYLOAD * 2)).unwrap();
        let mut assembler = Assembler::default();
        assert_eq!(assembler.accept(&fragments[0], 0), Ok(None));
        assert_eq!(assembler.accept(&fragments[0], 0), Err(Refused::Inconsistent));
        assert_eq!(assembler.waiting(), 0);

        let other = split(5, &message(MAX_PAYLOAD * 3)).unwrap();
        assert_eq!(assembler.accept(&fragments[0], 0), Ok(None));
        assert_eq!(assembler.accept(&other[1], 0), Err(Refused::Inconsistent), "three fragments, then two");
    }

    #[test]
    fn a_sender_cannot_park_messages_without_end() {
        let mut assembler = Assembler::default();
        for id in 0..MAX_PARTIAL as u32 {
            let fragments = split(id, &message(MAX_PAYLOAD + 1)).unwrap();
            assert_eq!(assembler.accept(&fragments[0], 0), Ok(None));
        }
        let one_more = split(99, &message(MAX_PAYLOAD + 1)).unwrap();
        assert_eq!(assembler.accept(&one_more[0], 0), Err(Refused::TooMany));
        // Left alone long enough, the half-arrived ones are forgotten and there is room again.
        assert_eq!(assembler.accept(&one_more[0], PATIENCE + 1), Ok(None));
        assert_eq!(assembler.waiting(), 1);
    }

    #[test]
    fn the_outbox_lets_a_few_fragments_out_each_tick_in_order() {
        let mut outbox = Outbox::default();
        let large = message(MAX_PAYLOAD * (PER_TICK + 3));
        outbox.push(&large).unwrap();
        outbox.push(b"after").unwrap();
        let mut assembler = Assembler::default();
        let mut arrived = Vec::new();
        let mut ticks = 0;
        while !outbox.is_empty() {
            let sent: Vec<_> = outbox.tick().collect();
            assert!(sent.len() <= PER_TICK);
            arrived.extend(sent.iter().filter_map(|fragment| assembler.accept(fragment, ticks).unwrap()));
            ticks += 1;
        }
        assert_eq!(ticks, 2, "eleven fragments and one more, eight a tick");
        assert_eq!(arrived, vec![large, b"after".to_vec()]);
        assert_eq!(outbox.push(&message(MAX_PAYLOAD * MAX_FRAGMENTS + 1)), Err(Refused::TooLarge));
    }
}
