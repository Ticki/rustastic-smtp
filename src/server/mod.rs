// Copyright 2014 The Rustastic SMTP Developers
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The `server` module contains things needed to build an SMTP server,
//! but useless for an SMTP client.

extern crate libc;

use super::common::stream::{InputStream, OutputStream};
use std::io::net::tcp::TcpStream;

extern {
    pub fn gethostname(name: *mut libc::c_char, size: libc::size_t) -> libc::c_int;
}

fn rust_gethostname() -> Result<String, ()> {
    let len = 255;
    let mut buf = Vec::<u8>::with_capacity(len);

    let ptr = buf.as_mut_slice().as_mut_ptr();

    let err = unsafe {
        gethostname(ptr as *mut libc::c_char, len as libc::size_t)
    } as int;

    match err {
        0 => {
            let mut real_len = len;
            let mut i = 0;
            loop {
                if i >= len {
                    break;
                }
                let byte = unsafe { *(((ptr as u64) + (i as u64)) as *const u8) };
                if byte == 0 {
                    real_len = i;
                    break;
                }

                i += 1;
            }
            unsafe { buf.set_len(real_len) }
            Ok(String::from_utf8_lossy(buf.as_slice()).into_owned())
        },
        _ => {
            Err(())
        }
    }
}

/// Gives access to the next middleware for a command.
pub struct Next<'a, CT, ST> {
    middleware: Middleware<'a, CT, ST>,
    next: Box<Option<Next<'a, CT, ST>>>
}

impl<'a, CT, ST> Next<'a, CT, ST> {
    /// Call a command middleware.
    pub fn call(&mut self, container: &mut CT, i: &mut InputStream<ST>, o: &mut OutputStream<ST>, l: &str) {
        let opt = match *self.next {
            Some(ref mut callback) => {
                Some(callback)
            },
            None => None
        };
        (self.middleware)(container, i, o, l, opt);
    }
}

/// A command middleware callback.
pub type Middleware<'a, CT, ST> = |
    &mut CT,
    &mut InputStream<ST>,
    &mut OutputStream<ST>,
    &str,
    Option<&mut Next<'a, CT, ST>>
|: 'a -> ();

/// An email server command.
///
/// It is defined by the string you find at the start of the command, for
/// example "MAIL FROM:" or "EHLO ", as well as a bunch of middleware parts
/// that are executed sequentially until one says to stop.
pub struct Command<'a, CT, ST> {
    start: Option<String>,
    middleware: Vec<Next<'a, CT, ST>>,
}

impl<'a, CT, ST> Command<'a, CT, ST> {
    /// Describes the start of the command line for this command.
    pub fn starts_with(&mut self, start: &str) {
        self.start = Some(start.into_string());
    }

    /// Add a middleware to call for this command.
    pub fn middleware(&mut self, middleware: Middleware<'a, CT, ST>) {
        self.middleware.push(Next {
            middleware: middleware,
            next: box None
        });
    }
}

/// An SMTP server, with no commands by default.
pub struct Server<'a, CT> {
    hostname: Option<String>,
    max_recipients: uint,
    max_message_size: uint,
    max_command_line_size: uint,
    max_text_line_size: uint,
    commands: Vec<Command<'a, CT, TcpStream>>,
    container: CT
}

// TODO: logging, via a Trait on the container?
// TODO: fatal error handling
// TODO: actual TCP listening and command handling

impl<'a, CT> Server<'a, CT> {
    /// Creates a new SMTP server.
    ///
    /// The container can be of any type and can be used to get access to a
    /// bunch of things inside your commands, like database connections,
    /// a logger and more.
    pub fn new(container: CT) -> Server<'a, CT> {
        Server {
            hostname: None,
            max_recipients: 100,
            max_message_size: 65536,
            max_command_line_size: 512,
            max_text_line_size: 1000,
            commands: Vec::with_capacity(16),
            container: container
        }
    }

    fn set_hostname(&mut self, hostname: &str) {
        self.hostname = Some(hostname.into_string());
    }

    fn set_max_recipients(&mut self, max: uint) {
        if max < 100 {
            panic!("Maximum number of recipients must be >= 100.");
        }
        self.max_recipients = max;
    }

    fn set_max_message_size(&mut self, max: uint) {
        if max < 65536 {
            panic!("Maximum message size must be >= 65536.");
        }
        self.max_message_size = max;
    }

    /// Adds a command to the server.
    pub fn add_command(&mut self, callback: |&mut Command<CT, TcpStream>| -> ()) {
        let mut command = Command {
            start: None,
            middleware: Vec::new(),
        };
        callback(&mut command);
    }

    fn increase_max_command_line_size(&mut self, bytes: uint) {
        self.max_command_line_size += bytes;
    }

    fn increase_max_text_line_size(&mut self, bytes: uint) {
        self.max_text_line_size += bytes;
    }

    /// Start the SMTP server on the given address.
    ///
    /// The address is something like "0.0.0.0:2525".
    pub fn listen(&mut self, address: &str) {
        if self.hostname == None {
            match rust_gethostname() {
                Ok(s) => {
                    self.hostname = Some(s);
                },
                Err(_) => {
                    panic!("Could not automatically get system hostname.");
                }
            }
        }
        println!("{} listening on {}", self.hostname, address);
    }
}
