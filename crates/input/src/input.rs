//! # User Input

#![no_std]

use spin_mutex::Mutex;

/// The number of [`InputEvent`]s an [`InputQueue`] can store at a time.
const QUEUE_SIZE: usize = 64;

/// The global user input queue.
pub static GLOBAL_INPUT_QUEUE: Mutex<InputQueue> = Mutex::new(InputQueue::new());

/// A user input event.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InputEvent {
    /// The user pressed a key.
    KeyPress { code: u16 },
    /// The user's mouse pointer moved.
    MouseMove { delta_x: i32, delta_y: i32 },
    /// The user's mouse wheel moved.
    MouseWheel { delta: i32 },
}

/// A queue of [`InputEvent`]s.
pub struct InputQueue {
    inner: [Option<InputEvent>; QUEUE_SIZE],
}

impl InputQueue {
    /// Create an empty input queue.
    const fn new() -> Self {
        Self {
            inner: [const { None }; QUEUE_SIZE],
        }
    }

    /// Push an [`InputEvent`] to the queue.
    pub fn push(&mut self, event: InputEvent) -> Option<InputEvent> {
        if let Some(null_index) = self.inner.iter().position(|event| event.is_none()) {
            self.inner[null_index] = Some(event);
            None
        } else {
            let missed_event = self.inner[0].take();
            self.inner.rotate_left(1);
            self.inner[QUEUE_SIZE - 1] = Some(event);
            missed_event
        }
    }

    /// Drain all [`InputEvent`]s from the queue.
    pub fn drain(&mut self) -> impl Iterator<Item = InputEvent> {
        self.inner.iter_mut().flat_map(|event| event.take())
    }
}
