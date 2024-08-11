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
use pc_keyboard::{layouts, DecodedKey, HandleControl, KeyCode, Keyboard, ScancodeSet1};
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
                    DecodedKey::RawKey(key) => match key {
                        KeyCode::ArrowLeft => {
                            push_char('\x1b');
                            push_char('<')
                        }
                        KeyCode::ArrowRight => {
                            push_char('\x1b');
                            push_char('>')
                        }
                        _ => {}
                    },
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
    let mut pos: usize = 0;
    let mut esc = false;
    print!("\x1bi");

    fn redraw_line(line: &str, pos: usize) {
        print!(
            "{}{}{}",
            "\x1b<".repeat(line.len()), // Move cursor to start
            line,                       // Redraw the line
            "\x1b<".repeat(pos)         // Move cursor back to the correct position
        );
    }

    fn clear_and_redraw(line: &str, pos: usize) {
        print!(
            "{}{}",
            "\x1b<".repeat(line.len()), // Move cursor to start
            " ".repeat(line.len()),     // Clear the line
        );
        redraw_line(line, pos);
    }

    loop {
        if let Some(character) = characters.next().await {
            if !esc && character != '\n' {
                if pos > 0 && pos < line.len() {
                    let mut temp_line = line.clone();
                    temp_line.insert(temp_line.len() - pos, character as char);
                    clear_and_redraw(&temp_line, pos);
                } else {
                    print!("{}", character);
                }
            }

            if esc {
                match character {
                    '<' | '>' => {
                        let (new_pos, move_cmd) = if character == '<' && pos < line.len() {
                            (pos + 1, "\x1b<")
                        } else if character == '>' && pos > 0 {
                            (pos - 1, "\x1b>")
                        } else {
                            continue;
                        };
                        pos = new_pos;
                        print!("{}", move_cmd);
                        redraw_line(&line, pos);
                    }
                    _ => {}
                }
                esc = false;
            } else {
                match character {
                    '\n' => {
                        redraw_line(&line, 0);
                        println!();
                        break;
                    }
                    '\u{8}' => {
                        if pos < line.len() {
                            line.remove((line.len() - pos) - 1);
                            clear_and_redraw(&line, pos);
                        }
                        continue;
                    }
                    '\x1b' => esc = true,
                    _ => {
                        if !esc {
                            line.insert(line.len() - pos, character as char);
                        }
                    }
                }
            }
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
