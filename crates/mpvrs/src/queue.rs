use std::time::{SystemTime, UNIX_EPOCH};

use crate::plumbing::audio::PlaybackMetadata;

#[derive(Clone, Debug)]
pub struct QueueItem {
    pub path: String,
    pub metadata: PlaybackMetadata,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum QueueSource {
    Folder(String),
    AllTracks,
    Artist(String),
    Album(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepeatMode {
    Off,
    One,
    Queue,
}

impl RepeatMode {
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::One,
            Self::One => Self::Queue,
            Self::Queue => Self::Off,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PlaybackQueue {
    #[allow(dead_code)]
    source: QueueSource,
    items: Vec<QueueItem>,
    current_index: usize,
    shuffle_enabled: bool,
    repeat_mode: RepeatMode,
    shuffled_order: Vec<usize>,
    shuffled_pos: usize,
    rng_state: u64,
}

impl PlaybackQueue {
    pub fn new(
        source: QueueSource,
        items: Vec<QueueItem>,
        start_path: &str,
        shuffle_enabled: bool,
        repeat_mode: RepeatMode,
    ) -> Self {
        let current_index = items
            .iter()
            .position(|item| item.path == start_path)
            .unwrap_or(0);
        let mut queue = Self {
            source,
            items,
            current_index,
            shuffle_enabled,
            repeat_mode,
            shuffled_order: Vec::new(),
            shuffled_pos: 0,
            rng_state: seed(),
        };
        queue.rebuild_shuffle_order();
        queue
    }

    pub fn current(&self) -> Option<&QueueItem> {
        self.items.get(self.current_index)
    }

    pub fn shuffle_enabled(&self) -> bool {
        self.shuffle_enabled
    }

    pub fn repeat_mode(&self) -> RepeatMode {
        self.repeat_mode
    }

    pub fn set_shuffle_enabled(&mut self, enabled: bool) {
        if self.shuffle_enabled == enabled {
            return;
        }
        self.shuffle_enabled = enabled;
        self.rebuild_shuffle_order();
    }

    pub fn cycle_repeat(&mut self) {
        self.repeat_mode = self.repeat_mode.next();
    }

    pub fn advance_after_finish(&mut self) -> Option<&QueueItem> {
        if self.repeat_mode == RepeatMode::One {
            return self.current();
        }
        self.step_forward(false)
    }

    pub fn next_manual(&mut self) -> Option<&QueueItem> {
        self.step_forward(true)
    }

    pub fn previous_manual(&mut self) -> Option<&QueueItem> {
        if self.items.is_empty() {
            return None;
        }

        if self.shuffle_enabled {
            if self.shuffled_pos > 0 {
                self.shuffled_pos -= 1;
                self.current_index = self.shuffled_order[self.shuffled_pos];
                return self.current();
            }
            if self.repeat_mode == RepeatMode::Queue {
                self.current_index = *self.shuffled_order.last().unwrap_or(&self.current_index);
                self.shuffled_pos = self.shuffled_order.len().saturating_sub(1);
                return self.current();
            }
            return None;
        }

        if self.current_index > 0 {
            self.current_index -= 1;
            return self.current();
        }
        if self.repeat_mode == RepeatMode::Queue {
            self.current_index = self.items.len().saturating_sub(1);
            return self.current();
        }
        None
    }

    fn step_forward(&mut self, manual: bool) -> Option<&QueueItem> {
        if self.items.is_empty() {
            return None;
        }

        if self.shuffle_enabled {
            if self.shuffled_pos + 1 < self.shuffled_order.len() {
                self.shuffled_pos += 1;
                self.current_index = self.shuffled_order[self.shuffled_pos];
                return self.current();
            }
            if manual || self.repeat_mode == RepeatMode::Queue {
                self.rebuild_shuffle_order();
                if self.shuffled_order.len() > 1 {
                    self.shuffled_pos = 1;
                    self.current_index = self.shuffled_order[self.shuffled_pos];
                } else {
                    self.shuffled_pos = 0;
                    self.current_index = self.shuffled_order[0];
                }
                return self.current();
            }
            return None;
        }

        if self.current_index + 1 < self.items.len() {
            self.current_index += 1;
            return self.current();
        }
        if manual || self.repeat_mode == RepeatMode::Queue {
            self.current_index = 0;
            return self.current();
        }
        None
    }

    fn rebuild_shuffle_order(&mut self) {
        self.shuffled_order.clear();
        if self.items.is_empty() {
            self.shuffled_pos = 0;
            return;
        }

        if !self.shuffle_enabled {
            self.shuffled_order.extend(0..self.items.len());
            self.shuffled_pos = self.current_index.min(self.items.len().saturating_sub(1));
            return;
        }

        self.shuffled_order.push(self.current_index);
        let mut rest: Vec<usize> = (0..self.items.len())
            .filter(|idx| *idx != self.current_index)
            .collect();
        shuffle_indices(&mut rest, &mut self.rng_state);
        self.shuffled_order.extend(rest);
        self.shuffled_pos = 0;
    }
}

fn shuffle_indices(indices: &mut [usize], state: &mut u64) {
    for i in (1..indices.len()).rev() {
        *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let j = ((*state >> 32) as usize) % (i + 1);
        indices.swap(i, j);
    }
}

fn seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0x9e37_79b9_7f4a_7c15)
}
