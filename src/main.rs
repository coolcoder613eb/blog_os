#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use alloc::{boxed::Box, rc::Rc, vec, vec::Vec};
use blog_os::task::{executor::Executor, keyboard, Task};
use blog_os::vga_buffer::{
    disable_cursor, enable_cursor, get_cursor_position, update_cursor, WRITER,
};
use blog_os::{allocator, serial_println};
use blog_os::{print, println, test_runner};
use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;

/// This function is called on panic.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("Aieee!! Kernel panic!\n{}", info);
    blog_os::hlt_loop();
}

entry_point!(kernel_main);
fn kernel_main(boot_info: &'static BootInfo) -> ! {
    use blog_os::memory;
    use blog_os::memory::BootInfoFrameAllocator;
    use x86_64::VirtAddr;

    blog_os::init();

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_map) };

    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

    #[cfg(test)]
    test_main();

    let mut executor = Executor::new();
    executor.spawn(Task::new(keyboard::save_keypresses()));
    executor.spawn(Task::new(shell()));
    executor.run();
}

#[test_case]
fn trivial_assertion() {
    assert_eq!(2 + 2, 4);
}

async fn shell() {
    // Clear screen
    print!("\x1bc");
    println!("\n    blog_os shell\n");
    enable_cursor();
    loop {
        print!(">");
        let line = keyboard::read_line().await;
        println!("Line: {}", line)
    }
}
