use crate::{print, serial_print};
use alloc::string::String;
use alloc::vec::Vec;
use conquer_once::spin::OnceCell;
use core::{
    pin::Pin,
    task::{Context, Poll},
};
use crossbeam_queue::ArrayQueue;
use futures_util::stream::{Stream, StreamExt};
use futures_util::task::AtomicWaker;
use lazy_static::lazy_static;
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
use spin::Mutex;

static WAKER: AtomicWaker = AtomicWaker::new();
static STDIN_WAKER: AtomicWaker = AtomicWaker::new();

static SCANCODE_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();

lazy_static! {
    static ref STDIN_QUEUE: Mutex<ArrayQueue<char>> = Mutex::new(ArrayQueue::new(256));
}
//static STDIN_BUFFER: Mutex<Vec<char>> = Mutex::new(Vec::new());

use crate::println;

/// Called by the keyboard interrupt handler
///
/// Must not block or allocate.
pub(crate) fn add_scancode(scancode: u8) {
    if let Ok(queue) = SCANCODE_QUEUE.try_get() {
        if let Err(_) = queue.push(scancode) {
            println!("WARNING: scancode queue full; dropping keyboard input");
        } else {
            WAKER.wake();
        }
    } else {
        println!("WARNING: scancode queue uninitialized");
    }
}

pub async fn save_keypresses() {
    let mut scancodes = ScancodeStream::new();
    let mut keyboard = Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore,
    );

    while let Some(scancode) = scancodes.next().await {
        if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
            if let Some(key) = keyboard.process_keyevent(key_event) {
                match key {
                    DecodedKey::Unicode(character) => push_char(character),
                    DecodedKey::RawKey(key) => serial_print!("{:?}", key),
                }
            }
        }
    }
}

fn push_char(character: char) {
    let queue = STDIN_QUEUE.lock();
    if let Err(_) = queue.push(character) {
        println!("WARNING: character queue full; dropping keyboard input");
    } else {
        STDIN_WAKER.wake();
    }
}

pub async fn read_line() -> String {
    let mut characters = InputStream {};
    let mut line = String::new();
    print!("\x1bi");

    loop {
        if let Some(character) = characters.next().await {
            print!("{}", character);
            match character {
                '\n' => break,
                '\u{8}' => {
                    line.pop();
                    continue;
                }
                _ => {}
            }
            line.push(character as char);
        }
    }
    print!("\x1bi");
    line
}

pub struct InputStream;

impl Stream for InputStream {
    type Item = char;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<char>> {
        let queue = STDIN_QUEUE.lock();

        // fast path
        if let Some(character) = queue.pop() {
            return Poll::Ready(Some(character));
        }

        STDIN_WAKER.register(&cx.waker());
        match queue.pop() {
            Some(character) => {
                STDIN_WAKER.take();
                Poll::Ready(Some(character))
            }
            None => Poll::Pending,
        }
    }
}

pub struct ScancodeStream {
    _private: (),
}

impl ScancodeStream {
    pub fn new() -> Self {
        SCANCODE_QUEUE
            .try_init_once(|| ArrayQueue::new(100))
            .expect("ScancodeStream::new should only be called once");
        ScancodeStream { _private: () }
    }
}

impl Stream for ScancodeStream {
    type Item = u8;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<u8>> {
        let queue = SCANCODE_QUEUE
            .try_get()
            .expect("scancode queue not initialized");

        // fast path
        if let Some(scancode) = queue.pop() {
            return Poll::Ready(Some(scancode));
        }

        WAKER.register(&cx.waker());
        match queue.pop() {
            Some(scancode) => {
                WAKER.take();
                Poll::Ready(Some(scancode))
            }
            None => Poll::Pending,
        }
    }
}
