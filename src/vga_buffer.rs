use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;
use volatile::Volatile;
use x86_64::instructions::port::Port;

use crate::serial_print;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct ColorCode(u8);

impl ColorCode {
    fn new(foreground: Color, background: Color) -> ColorCode {
        ColorCode((background as u8) << 4 | (foreground as u8))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
}

const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;

#[repr(transparent)]
struct Buffer {
    chars: [[Volatile<ScreenChar>; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

struct Pos {
    pos: usize,
}
impl Pos {
    fn new(col: usize, row: usize) -> Self {
        Self {
            pos: row * BUFFER_WIDTH + col,
        }
    }
    fn col(&self) -> usize {
        self.pos % BUFFER_WIDTH
    }

    fn row(&self) -> usize {
        self.pos / BUFFER_WIDTH
    }
}

pub struct Writer {
    position: Pos,
    mutable_start: Pos,
    mutable: bool,
    color_code: ColorCode,
    buffer: &'static mut Buffer,
}

impl Writer {
    pub fn write_byte(&mut self, byte: u8, esc: bool) {
        match byte {
            b'\n' => self.new_line(),
            8 => {
                if self.mutable && self.position.pos > self.mutable_start.pos {
                    serial_print!("B {} ", self.position.pos);
                    self.position.pos -= 1;
                    let row = self.position.row();
                    let col = self.position.col();
                    let color_code = self.color_code;
                    self.buffer.chars[row][col].write(ScreenChar {
                        ascii_character: b' ',
                        color_code,
                    });
                }
            }
            byte => {
                if esc {
                    match byte {
                        b'i' => {
                            self.mutable = !self.mutable;
                            self.mutable_start.pos = self.position.pos;
                        }
                        b'c' => self.clear_screen(),
                        _ => {}
                    }
                } else {
                    if self.position.col() >= BUFFER_WIDTH {
                        self.new_line();
                    }

                    let row = self.position.row();
                    let col = self.position.col();

                    let color_code = self.color_code;
                    self.buffer.chars[row][col].write(ScreenChar {
                        ascii_character: byte,
                        color_code,
                    });
                    self.position.pos += 1;
                }
            }
        }
        update_cursor(self.position.pos as u16);
    }

    pub fn write_string(&mut self, s: &str) {
        let mut esc = false;
        for byte in s.bytes() {
            match byte {
                // printable ASCII byte or newline or backspace
                0x20..=0x7e | b'\n' | 8 => self.write_byte(byte, esc),
                0x1b => {
                    esc = true;
                    continue;
                }
                // not part of printable ASCII range
                _ => self.write_byte(0xfe, esc),
            }
            esc = false;
        }
    }

    fn new_line(&mut self) {
        if self.position.row() == BUFFER_HEIGHT - 1 {
            for row in 1..BUFFER_HEIGHT {
                for col in 0..BUFFER_WIDTH {
                    let character = self.buffer.chars[row][col].read();
                    self.buffer.chars[row - 1][col].write(character);
                }
            }
            self.clear_row(BUFFER_HEIGHT - 1);
            self.position.pos -= self.position.pos - (BUFFER_WIDTH * self.position.row());
        } else {
            self.position.pos += BUFFER_WIDTH;
            self.position.pos -= self.position.pos - (BUFFER_WIDTH * self.position.row());
        }
    }

    fn clear_screen(&mut self) {
        for row in 0..BUFFER_HEIGHT {
            self.clear_row(row)
        }
    }

    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            ascii_character: b' ',
            color_code: self.color_code,
        };
        for col in 0..BUFFER_WIDTH {
            self.buffer.chars[row][col].write(blank);
        }
    }
}

fn outb(id: u16, val: u8) {
    unsafe {
        Port::new(id).write(val);
    }
}
fn inb(id: u16) -> u8 {
    unsafe {
        let input: u8 = Port::new(id).read();
        input
    }
}
pub fn disable_cursor() {
    outb(0x3D4, 0x0A);
    outb(0x3D5, 0x20);
}

pub fn enable_cursor() {
    outb(0x3D4, 0x0A);
    outb(0x3D5, (inb(0x3D5) & 0xC0) | 13);

    outb(0x3D4, 0x0B);
    outb(0x3D5, (inb(0x3D5) & 0xE0) | 15);
}
pub fn update_cursor(pos: u16) {
    //(x: usize, y: usize) {
    //let pos: u16 = (position.row() * BUFFER_WIDTH + position.col()) as u16;

    outb(0x3D4, 0x0F);
    outb(0x3D5, (pos & 0xFF) as u8);
    outb(0x3D4, 0x0E);
    outb(0x3D5, ((pos >> 8) & 0xFF) as u8);
}
pub fn get_cursor_position() -> u16 {
    let mut pos: u16 = 0;
    outb(0x3D4, 0x0F);
    pos |= inb(0x3D5) as u16;
    outb(0x3D4, 0x0E);
    pos |= (inb(0x3D5) as u16) << 8;
    return pos;
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

lazy_static! {
    pub static ref WRITER: Mutex<Writer> = Mutex::new(Writer {
        position: Pos::new(0, 0),
        mutable_start: Pos::new(0, 0),
        mutable: false,
        color_code: ColorCode::new(Color::Yellow, Color::Black),
        buffer: unsafe { &mut *(0xb8000 as *mut Buffer) },
    });
}
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::vga_buffer::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        WRITER.lock().write_fmt(args).unwrap();
    });
}

#[test_case]
fn test_println_simple() {
    println!("test_println_simple output");
}
#[test_case]
fn test_println_many() {
    for _ in 0..200 {
        println!("test_println_many output");
    }
}
#[test_case]
fn test_println_output() {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    let s = "Some test string that fits on a single line";
    interrupts::without_interrupts(|| {
        let mut writer = WRITER.lock();
        writeln!(writer, "\n{}", s).expect("writeln failed");
        for (i, c) in s.chars().enumerate() {
            let screen_char = writer.buffer.chars[BUFFER_HEIGHT - 2][i].read();
            assert_eq!(char::from(screen_char.ascii_character), c);
        }
    });
}
