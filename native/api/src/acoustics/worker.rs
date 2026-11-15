//! The rays are cast on a thread of their own: a few thousand block lookups per sound is
//! nothing for a spare core and too much for the frame.
//!
//! The frame hands over a snapshot of the chunks around the player (the game shares them,
//! nothing is copied) and where the sounds are; the thread answers when it is done, and is
//! only asked again then. A place does not change faster than a few answers a second, and
//! the frame glides between them (`solve::Heard::toward`).
use super::materials::{self, Material};
use super::solve::{Heard, REACH, evaluate};
use super::trace::{Pos, Vec3, World};
use horizon_client::mod_api::{Host, NativeClientChunks};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};

/// One sound to work out: the emitter it is for, and where it is.
#[derive(Clone, Copy, Debug)]
pub struct Job {
    pub id: u32,
    pub source: Vec3,
}

/// What every block state does to sound, by state id.
pub type Table = Arc<[Option<Material>]>;

/// The table for the block states this game has now.
pub fn table(host: &dyn Host) -> Table {
    let states = host.block_states();
    states
        .iter()
        .map(|(_, facts)| {
            let whole = facts.outline.len() == 1 && facts.outline[0] == [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
            if facts.air { None } else { materials::of(&facts.name, facts.collides, whole) }
        })
        .collect()
}

struct Request {
    listener: Vec3,
    jobs: Vec<Job>,
    chunks: NativeClientChunks,
    table: Table,
}

/// The blocks of a snapshot, as the rays see them.
struct Blocks<'a> {
    chunks: &'a NativeClientChunks,
    table: &'a [Option<Material>],
}

impl World for Blocks<'_> {
    fn at(&self, pos: Pos) -> Option<Material> {
        *self.table.get(self.chunks.block(pos).0 as usize)?
    }
}

pub struct Worker {
    requests: Sender<Request>,
    answers: Arc<Mutex<HashMap<u32, Heard>>>,
    busy: Arc<AtomicBool>,
}

impl Default for Worker {
    fn default() -> Self {
        Self::new()
    }
}

impl Worker {
    /// Starts the thread. It ends when the worker is dropped.
    pub fn new() -> Self {
        let (requests, inbox) = channel::<Request>();
        let answers = Arc::new(Mutex::new(HashMap::new()));
        let busy = Arc::new(AtomicBool::new(false));
        let (shared, working) = (answers.clone(), busy.clone());
        let spawned = std::thread::Builder::new().name("webdisplays-acoustics".to_owned()).spawn(move || {
            while let Ok(request) = inbox.recv() {
                let world = Blocks { chunks: &request.chunks, table: &request.table };
                let heard: HashMap<u32, Heard> =
                    request.jobs.iter().map(|job| (job.id, evaluate(&world, job.source, request.listener))).collect();
                if let Ok(mut answers) = shared.lock() {
                    *answers = heard;
                }
                working.store(false, Ordering::Release);
            }
        });
        if spawned.is_err() {
            // No thread to be had: the worker stays busy for ever, and every sound dry.
            busy.store(true, Ordering::Release);
        }
        Self { requests, answers, busy }
    }

    /// Asks for `jobs` as heard from `listener`, unless the last request is still being
    /// worked on. The chunks within a ray's reach of the listener are snapshotted now.
    pub fn ask(&self, host: &dyn Host, table: &Table, listener: Vec3, jobs: Vec<Job>) {
        if jobs.is_empty() || self.busy.load(Ordering::Acquire) {
            return;
        }
        let chunk = |coordinate: f64, by: f64| ((coordinate + by).floor() as i32) >> 4;
        let from = [chunk(listener[0], -REACH), chunk(listener[2], -REACH)];
        let to = [chunk(listener[0], REACH), chunk(listener[2], REACH)];
        let Ok(chunks) = host.client_chunks(from, to) else {
            return;
        };
        self.busy.store(true, Ordering::Release);
        if self.requests.send(Request { listener, jobs, chunks, table: table.clone() }).is_err() {
            self.busy.store(false, Ordering::Release);
        }
    }

    /// The last answers: what each sound asked about is heard as.
    pub fn answers(&self) -> HashMap<u32, Heard> {
        self.answers.lock().map(|answers| answers.clone()).unwrap_or_default()
    }
}
