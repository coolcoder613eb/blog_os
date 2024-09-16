#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use alloc::{boxed::Box, rc::Rc, string::String, vec, vec::Vec};
use blog_os::ata::{get_disks, init_ata, read_data};
use blog_os::simplefs::unpack;
use blog_os::task::{executor::Executor, keyboard, Task};
use blog_os::vga_buffer::{
    disable_cursor, enable_cursor, get_cursor_position, update_cursor, WRITER,
};
use blog_os::wasm::wasm_runner;
use blog_os::{allocator, serial_println};
use blog_os::{print, println, test_runner};
use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use shlex::split;

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

    init_ata();

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
        let maybe_command = split(line.as_str());
        let mut command: Vec<String> = vec![];
        match maybe_command {
            Some(cmd) => {
                command = cmd;
            }
            None => {
                println!("{}: Invalid command!", line)
            }
        }
        if command.len() > 0 {
            match command[0].as_str() {
                "ls" => {
                    let fs = read_data(0, 1, 0, 2048);
                    let files = unpack(fs);
                    for file in files {
                        println!("{}", file.0)
                    }
                }
                "xyzzy" => println!("Nothing happens."),
                "echo" => println!("{}", command[1..].join(" ")),
                "disks" => get_disks(),
                "run" => {
                    if command.len() > 1 {
                        let fs = read_data(0, 1, 0, 2048); // bus 0, disk 1, from 0, 2048 blocks (1M)
                        let files = unpack(fs);
                        let mut found = false;
                        for file in files {
                            if file.0.starts_with(command[1].as_str()) && file.0.ends_with(".wasm")
                            {
                                println!(
                                    "Program finished with exit code {:?}.",
                                    wasm_runner(file.1)
                                );
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            println!("Program not found.")
                        }
                    }
                }
                _ => {
                    println!("{}: Unknown command!", command[0])
                }
            }
        }
    }
}
