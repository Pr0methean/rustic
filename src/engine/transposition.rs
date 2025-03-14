/* =======================================================================
Rustic is a chess playing engine.
Copyright (C) 2019-2024, Marcel Vanthoor
https://rustic-chess.org/

Rustic is written in the Rust programming language. It is an original
work, not derived from any engine that came before it. However, it does
use a lot of concepts which are well-known and are in use by most if not
all classical alpha/beta-based chess engines.

Rustic is free software: you can redistribute it and/or modify it under
the terms of the GNU General Public License version 3 as published by
the Free Software Foundation.

Rustic is distributed in the hope that it will be useful, but WITHOUT
ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or
FITNESS FOR A PARTICULAR PURPOSE.  See the GNU General Public License
for more details.

You should have received a copy of the GNU General Public License along
with this program.  If not, see <http://www.gnu.org/licenses/>.
======================================================================= */
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};
use parking_lot::RwLock;
use dashmap::DashMap;
use dashmap::Entry::Occupied;
use explicit_cast::{Truncate, TruncateFrom};
use smallvec::{smallvec, SmallVec};
use crate::{board::defs::ZobristKey, movegen::defs::ShortMove, search::defs::CHECKMATE_THRESHOLD};
use crate::board::Board;

const MEGABYTE: usize = 1024 * 1024;
const ENTRIES_PER_BUCKET: usize = 4;
const BUCKETS_FOR_PARTIAL_HASH: usize = 1 << 32;
const MIN_BUCKETS_PER_TABLE: usize = 1;
const EXPANSION_FACTORS: [usize; 1] = [2];

/* ===== Data ========================================================= */

pub trait IHashData {
    fn new() -> Self;
    fn depth(&self) -> i8;
}
#[derive(Copy, Clone)]
pub struct PerftData {
    depth: i8,
    leaf_nodes: u64,
}

impl IHashData for PerftData {
    fn new() -> Self {
        Self {
            depth: 0,
            leaf_nodes: 0,
        }
    }

    fn depth(&self) -> i8 {
        self.depth
    }
}

impl PerftData {
    pub fn create(depth: i8, leaf_nodes: u64) -> Self {
        Self { depth, leaf_nodes }
    }

    pub fn get(&self, depth: i8) -> Option<u64> {
        if self.depth == depth {
            Some(self.leaf_nodes)
        } else {
            None
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum HashFlag {
    Nothing,
    Exact,
    Alpha,
    Beta,
}

#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(packed)]
pub struct SearchData {
    depth: i8,
    flag: HashFlag,
    value: i16,
    best_move: ShortMove,
}

impl IHashData for SearchData {
    fn new() -> Self {
        Self {
            depth: 0,
            flag: HashFlag::Nothing,
            value: 0,
            best_move: ShortMove::new(0),
        }
    }

    fn depth(&self) -> i8 {
        self.depth
    }
}

impl SearchData {
    pub fn create(depth: i8, ply: i8, flag: HashFlag, value: i16, best_move: ShortMove) -> Self {
        // This is the value we're going to save into the TT.
        let mut v = value;

        // If we're dealing with checkmate, the value must be adjusted, so
        // they take the number of plies at which they were found into
        // account, before storing the value into the TT. These ifs can be
        // rewritten as a comparative match expression. We don't, because
        // they're slower. (No inlining by the compiler.)
        if v > CHECKMATE_THRESHOLD {
            v += ply as i16;
        }

        if v < CHECKMATE_THRESHOLD {
            v -= ply as i16;
        }

        Self {
            depth,
            flag,
            value: v,
            best_move,
        }
    }

    pub fn get(&self, depth: i8, ply: i8, alpha: i16, beta: i16) -> (Option<i16>, ShortMove) {
        // We either do, or don't have a value to return from the TT.
        let mut value: Option<i16> = None;

        if self.depth >= depth {
            match self.flag {
                HashFlag::Exact => {
                    // Get the value from the data. We don't want to change
                    // Get the value from the data. We don't want to change
                    // the value that is in the TT.
                    let mut v = self.value;

                    // Adjust for the number of plies from where this data
                    // is probed, if we're dealing with checkmate. Same as
                    // above: no comparative match expression.
                    if v > CHECKMATE_THRESHOLD {
                        v -= ply as i16;
                    }

                    if v < CHECKMATE_THRESHOLD {
                        v += ply as i16;
                    }

                    // This is the value that will be returned.
                    value = Some(v);
                }
                HashFlag::Alpha => {
                    if self.value <= alpha {
                        value = Some(alpha);
                    }
                }
                HashFlag::Beta => {
                    if self.value >= beta {
                        value = Some(beta);
                    }
                }
                _ => (),
            };
        }
        (value, self.best_move)
    }
}

/* ===== Entry ======================================================== */

#[derive(Copy, Clone, Eq, PartialEq)]
struct Entry<V: Copy, D> {
    verification: V,
    data: D,
}

type RehashableEntry<D> = Entry<u64, D>;
type NonRehashableEntry<D> = Entry<u32, D>;

impl <V: Copy + TruncateFrom<u128>, D: IHashData> Entry<V, D> {
    fn new() -> Self {
        Self {
            verification: 0.truncate(),
            data: D::new(),
        }
    }
}

/* ===== Bucket ======================================================= */

#[derive(Clone, Eq, PartialEq)]
struct Bucket<E> {
    bucket: [E; ENTRIES_PER_BUCKET],
}

type RehashableBucket<D> = Bucket<RehashableEntry<D>>;
type NonRehashableBucket<D> = Bucket<NonRehashableEntry<D>>;

impl<D: IHashData + Copy, V: Eq + Copy> Bucket<Entry<V, D>> where V: TruncateFrom<u128> {
    fn new() -> Self {
        Self {
            bucket: [Entry::new(); ENTRIES_PER_BUCKET],
        }
    }

    fn store(&mut self, verification: u64, data: D, used_entries: &mut usize, overwrite: bool) -> bool {
        let mut idx_lowest_depth = 0;

        // Find the index of the entry with the lowest depth.
        for entry in 1..ENTRIES_PER_BUCKET {
            if self.bucket[entry].data.depth() < data.depth() {
                idx_lowest_depth = entry
            }
        }

        // If the verifiaction was 0, this entry in the bucket was never
        // used before. Count the use of this entry.
        if self.bucket[idx_lowest_depth].verification == 0.truncate() {
            *used_entries += 1;
        } else if !overwrite {
            // If the entry was used before, and we're not overwriting
            // the entry, return false.
            return false;
        }

        // Store.
        self.bucket[idx_lowest_depth] = Entry { verification: (verification as u128).truncate(), data };
        true
    }

    fn find(&self, verification: u64) -> Option<&D> {
        let verification = (verification as u128).truncate();
        for e in self.bucket.iter() {
            if e.verification == verification {
                return Some(&e.data);
            }
        }
        None
    }

    fn find_mut(&mut self, verification: u64) -> Option<&mut D> {
        let verification = (verification as u128).truncate();
        for e in self.bucket.iter_mut() {
            if e.verification == verification {
                return Some(&mut e.data);
            }
        }
        None
    }
}

/* ===== TT =================================================== */

// Transposition Table
#[derive(Eq, PartialEq, Clone)]
enum TTCore<D> {
    FullHash(SmallVec<[RehashableBucket<D>; MIN_BUCKETS_PER_TABLE]>),
    HalfHash(Vec<NonRehashableBucket<D>>),
}

impl<D> TTCore<D> {
    pub(crate) fn len(&self) -> usize {
        match self {
            TTCore::FullHash(ref tt) => tt.len(),
            TTCore::HalfHash(ref tt) => tt.len(),
        }
    }

    pub(crate) fn size_bytes(&self) -> usize {
        size_of::<Self>() + match self {
            TTCore::FullHash(ref tt) => (tt.len().saturating_sub(tt.inline_size())) * std::mem::size_of::<RehashableBucket<D>>(),
            TTCore::HalfHash(ref tt) => tt.len() * std::mem::size_of::<NonRehashableBucket<D>>(),
        }
    }

    // Calculate the index (bucket) where the data is going to be stored.
    // Use only the upper half of the Zobrist key for this, so the lower
    // half can be used to calculate a verification.
    fn calculate_index(&self, zobrist_key: ZobristKey) -> usize {
        zobrist_key as usize % self.len()
    }
}

#[derive(Eq, PartialEq, Clone)]
pub struct TT<D> {
    tt: TTCore<D>,
    used_entries: usize,
}

// Public functions
impl<D: IHashData + Copy + Clone> TT<D> {
    // Create a new TT of the requested size, able to hold the data
    // of type D, where D has to implement IHashData, and must be clonable
    // and copyable.
    pub fn new(megabytes: usize) -> Self {
        let total_buckets = Self::calculate_init_buckets(megabytes);

        Self::new_with_buckets(total_buckets)
    }

    fn new_with_buckets(buckets: usize) -> TT<D> {
        if buckets >= BUCKETS_FOR_PARTIAL_HASH {
            Self {
                tt: TTCore::FullHash(smallvec![RehashableBucket::<D>::new(); buckets]),
                used_entries: 0
            }
        } else {
            Self {
                tt: TTCore::HalfHash(vec![NonRehashableBucket::<D>::new(); buckets]),
                used_entries: 0
            }
        }
    }

    pub(crate) fn size_bytes(&self) -> usize {
        (size_of::<Self>() - size_of::<TTCore<D>>()) + self.tt.size_bytes()
    }

    // Resizes the TT by replacing the current TT with a
    // new one. (We don't use Vec's resize function, because it clones
    // elements. This can be problematic if TT sizes push the
    // computer's memory limits.)
    pub fn resize(&mut self, megabytes: usize, room_to_grow: &AtomicIsize) {
        let total_buckets = TT::<D>::calculate_init_buckets(megabytes);

        self.resize_to_bucket_count(total_buckets, room_to_grow);
    }

    fn resize_to_bucket_count(&mut self, buckets: usize, room_to_grow: &AtomicIsize) -> bool {
        let old_bucket_count = self.tt.len();
        let old_size_bytes = self.tt.size_bytes();
        let new_size_bytes = size_of::<TTCore<D>>() + if buckets >= BUCKETS_FOR_PARTIAL_HASH {
            buckets * std::mem::size_of::<NonRehashableBucket<D>>()
        } else {
            (buckets - MIN_BUCKETS_PER_TABLE) * std::mem::size_of::<RehashableBucket<D>>()
        };
        if new_size_bytes > old_size_bytes {
            let bytes_added = (new_size_bytes - old_size_bytes) as isize;
            if room_to_grow.fetch_sub(bytes_added, Ordering::AcqRel) - bytes_added < 0 {
                room_to_grow.fetch_add(bytes_added, Ordering::AcqRel);
                return false;
            }
        } else {
            room_to_grow.fetch_add((old_size_bytes - new_size_bytes) as isize, Ordering::AcqRel);
        }
        if buckets >= BUCKETS_FOR_PARTIAL_HASH {
            self.tt = TTCore::HalfHash(vec![NonRehashableBucket::<D>::new(); buckets]);
            self.used_entries = 0;
        } else if buckets > old_bucket_count {
            if let TTCore::FullHash(ref mut tt) = self.tt {
                tt.resize(buckets, RehashableBucket::<D>::new());
                let (old_buckets, new_buckets) = tt.split_at_mut(old_bucket_count);
                for (index, bucket) in old_buckets.iter_mut().enumerate() {
                    for entry in bucket.bucket.iter_mut() {
                        if entry.verification != 0 {                            let zobrist_key = entry.verification;
                            let new_index = zobrist_key as usize % buckets;
                            if new_index != index {
                                debug_assert!(new_index > index, "rehashing from bucket {} of {} to bucket {} of {}",
                                              index, old_bucket_count, new_index, buckets);
                                debug_assert!(new_index - old_bucket_count < new_buckets.len(),
                                              "rehashing from bucket {} of {} to bucket {} of {}",
                                              index, old_bucket_count, new_index, buckets);
                                new_buckets[new_index - old_bucket_count].store(entry.verification, entry.data, &mut self.used_entries, false);
                                entry.clone_from(&RehashableEntry::new());
                            }
                        }
                    }
                }
                return true;
            }
        }
        self.tt = TTCore::FullHash(smallvec![RehashableBucket::<D>::new(); buckets]);
        self.used_entries = 0;
        true
    }

    // Insert a position at the calculated index, by storing it in the
    // index's bucket.
    pub fn insert(&mut self, zobrist_key: ZobristKey, data: D, room_to_grow: &AtomicIsize) {
        if self.tt.len() > 0 {
            let verification = self.calculate_verification(zobrist_key);
            'try_store_or_grow: while let TTCore::FullHash(ref mut tt) = self.tt {
                let index = zobrist_key as usize % tt.len();
                if tt[index].store(verification, data, &mut self.used_entries, false) {
                    return;
                }
                for expansion_factor in EXPANSION_FACTORS {
                    let new_bucket_count = self.tt.len().checked_mul(expansion_factor);
                    if let Some(new_bucket_count) = new_bucket_count {
                        if self.resize_to_bucket_count(new_bucket_count, room_to_grow) {
                            continue 'try_store_or_grow;
                        }
                    }
                    break 'try_store_or_grow;
                }
                break;
            }
            let index = zobrist_key as usize % self.tt.len();
            match self.tt {
                TTCore::FullHash(ref mut tt) => tt[index].store(verification, data, &mut self.used_entries, true),
                TTCore::HalfHash(ref mut tt) => tt[index].store(verification, data, &mut self.used_entries, true),
            };
        }
    }

    // Probe the TT by both verification and depth. Both have to
    // match for the position to be the correct one we're looking for.
    pub fn probe(&self, zobrist_key: ZobristKey) -> Option<&D> {
        if self.tt.len() > 0 {
            let index = self.tt.calculate_index(zobrist_key);
            let verification = self.calculate_verification(zobrist_key);
            match self.tt {
                TTCore::FullHash(ref tt) => tt[index].find(verification),
                TTCore::HalfHash(ref tt) => tt[index].find(verification),
            }
        } else {
            None
        }
    }

    pub fn probe_mut(&mut self, zobrist_key: ZobristKey) -> Option<&mut D> {
        if self.tt.len() > 0 {
            let index = self.tt.calculate_index(zobrist_key);
            let verification = self.calculate_verification(zobrist_key);
            match &mut self.tt{
                TTCore::FullHash(ref mut tt) => tt[index].find_mut(verification),
                TTCore::HalfHash(ref mut tt) => tt[index].find_mut(verification)
            }
        } else {
            None
        }
    }

    // Clear TT by replacing it with a new one.
    pub fn clear(&mut self) {
        self.tt = TTCore::FullHash(smallvec![RehashableBucket::<D>::new()]);
        self.used_entries = 0;
    }

    // Provides TT usage in permille (1 per 1000, as oppposed to percent,
    // which is 1 per 100.)
    pub fn hash_full(&self) -> u16 {
        if self.tt.len() > 0 {
            ((self.used_entries as f64 / (self.tt.len() * ENTRIES_PER_BUCKET) as f64) * 1000f64).floor() as u16
        } else {
            0
        }
    }
}

// Private functions
impl<D: IHashData + Copy + Clone> TT<D> {
    // Many positions will end up at the same index, and thus in the same
    // bucket. Calculate a verification for the position so it can later be
    // found in the bucket. Use the other half of the Zobrist key for this.
    fn calculate_verification(&self, zobrist_key: ZobristKey) -> u64 {
        zobrist_key
    }

    // This function calculates the value for total_buckets depending on the
    // requested TT size.
    fn calculate_init_buckets(megabytes: usize) -> usize {
        megabytes * MEGABYTE / size_of::<RehashableBucket<D>>()
    }
}

type OurMap = DashMap<u32, RwLock<TT<SearchData>>>;

pub struct TTree {
    map: OurMap,
    max_size: AtomicUsize,
    room_to_grow: AtomicIsize
}

impl TTree {
    pub fn new(size_mb: usize) -> Self {
        let size_bytes = size_mb * MEGABYTE;
        Self {
            map: DashMap::new(),
            max_size: AtomicUsize::new(size_bytes),
            room_to_grow: AtomicIsize::new(size_bytes as isize)
        }
    }

    fn get_map(&self) -> &OurMap {
        &self.map
    }

    pub fn size_bytes(&self) -> usize {
        size_of::<Self>() + self.get_map()
            .iter()
            .map(|v| v.value().read().size_bytes() + size_of::<u32>())
            .sum::<usize>()
    }

    pub fn insert(&self, board: &Board, value: SearchData) {
        let zobrist_key = board.game_state.zobrist_key;
        let entry = self.get_map().entry(board.monotonic_hash());
        match &entry {
            Occupied(ref e) => {
                e.get().write().insert(zobrist_key, value, &self.room_to_grow);
            },
            _ => {
                let mut new_buckets: SmallVec<[RehashableBucket<SearchData>; MIN_BUCKETS_PER_TABLE]> = smallvec![RehashableBucket::<SearchData>::new(); MIN_BUCKETS_PER_TABLE];
                let index = zobrist_key as usize % MIN_BUCKETS_PER_TABLE;
                new_buckets[index].bucket[0].verification = zobrist_key;
                new_buckets[index].bucket[0].data = value;
                let new_table = TT {
                    tt: TTCore::FullHash(new_buckets),
                    used_entries: 1
                };
                let new_table_size = new_table.tt.size_bytes() as isize;
                if self.room_to_grow.fetch_sub(new_table_size, Ordering::AcqRel) - new_table_size < 0 {
                    self.room_to_grow.fetch_add(new_table_size, Ordering::AcqRel);
                    return;
                }
                entry.insert(RwLock::new(new_table));
            }
        }
    }

    pub fn probe(&self, board: &Board) -> Option<SearchData> {
        self.get_map().get(&board.monotonic_hash())?.read().probe(board.game_state.zobrist_key).cloned()
    }

    pub fn hash_full(&self) -> u16 {
        let max_size = self.max_size.load(Ordering::Acquire);
        let current_size = self.get_map()
            .iter()
            .map(|v| size_of::<u32>() + size_of::<NonRehashableEntry<SearchData>>() * v.value().read().used_entries)
            .sum::<usize>();
        ((current_size * 1000 + 500) / max_size) as u16
    }

    pub fn remove_unreachable(&self, new_monotonic_hash: u32) {
        self.map.retain(|k, _| k <= &new_monotonic_hash);
        self.recalculate_room_to_grow();
    }

    fn recalculate_room_to_grow(&self) -> isize {
        let new_room_to_grow = self.max_size.load(Ordering::SeqCst) as isize - self.size_bytes() as isize;
        self.room_to_grow.store(new_room_to_grow, Ordering::SeqCst);
        new_room_to_grow
    }

    pub fn clear(&self) {
        self.get_map().clear();
        self.room_to_grow.store(self.max_size.load(Ordering::SeqCst) as isize, Ordering::SeqCst);
    }

    pub fn resize(&self, megabytes: usize) {
        let new_max_size = megabytes * MEGABYTE;
        let mut size_change: isize;
        loop {
            let old_max_size = self.max_size.load(Ordering::Acquire);
            size_change = new_max_size as isize - old_max_size as isize;
            if self.max_size.compare_exchange(old_max_size, new_max_size, Ordering::SeqCst, Ordering::Acquire).is_ok() {
                break;
            }
        }
        let mut new_room_to_grow = self.room_to_grow.fetch_add(size_change, Ordering::SeqCst) + size_change;
        while new_room_to_grow < 0 {
            let max_buckets = self.get_map().iter().map(
                |tt| tt.value().read().tt.len()).max().unwrap();
            let new_max_buckets = max_buckets / 2;
            if new_max_buckets < 1 {
                return;
            }
            let mut bytes_freed = 0;
            for tt in self.get_map().iter() {
                let mut tt = tt.value().write();
                if tt.tt.len() > new_max_buckets {
                    let old_size = tt.tt.size_bytes();
                    tt.resize_to_bucket_count(new_max_buckets, &self.room_to_grow);
                    bytes_freed += (old_size - tt.tt.size_bytes()) as isize;
                }
            }
            new_room_to_grow += bytes_freed;
            self.room_to_grow.fetch_add(bytes_freed, Ordering::SeqCst);
        }
    }
}
