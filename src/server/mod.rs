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
use std::io::net::tcp::{TcpListener, TcpAcceptor, TcpStream};
use std::io::net::ip::{SocketAddr, IpAddr, Port};
use std::io::{Acceptor, Listener, IoResult};
use std::sync::Arc;

/// Core SMTP commands
pub mod commands;

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
pub struct Next<CT, ST> {
    middleware: Middleware<CT, ST>,
    next: Box<Option<Next<CT, ST>>>
}

impl<CT, ST> Clone for Next<CT, ST> {
    fn clone(&self) -> Next<CT, ST> {
        Next {
            middleware: self.middleware,
            next: box() (*self.next.clone())
        }
    }
}

impl<CT, ST> Next<CT, ST> {
    /// Call a command middleware.
    pub fn call(&mut self, container: &mut CT, i: &mut InputStream<ST>, o: &mut OutputStream<ST>, l: &str) {
        let opt = match *self.next {
            Some(ref next) => Some(next.clone()),
            None => None
        };
        (self.middleware)(container, i, o, l, opt);
    }
}

/// A command middleware callback.
pub type Middleware<CT, ST> = fn(
    &mut CT,
    &mut InputStream<ST>,
    &mut OutputStream<ST>,
    &str,
    Option<Next<CT, ST>>
) -> ();

/// An email server command.
///
/// It is defined by the string you find at the start of the command, for
/// example "MAIL FROM:" or "EHLO ", as well as a bunch of middleware parts
/// that are executed sequentially until one says to stop.
#[deriving(Clone)]
pub struct Command<CT, ST> {
    start: Option<String>,
    middleware: Vec<Next<CT, ST>>,
}

impl<CT, ST> Command<CT, ST> {
    /// Creates a new command
    pub fn new() -> Command<CT, ST> {
        Command {
            start: None,
            middleware: Vec::new()
        }
    }

    /// Describes the start of the command line for this command.
    pub fn starts_with(&mut self, start: &str) {
        self.start = Some(start.into_string());
    }

    /// Add a middleware to call for this command.
    pub fn middleware(&mut self, middleware: Middleware<CT, ST>) {
        self.middleware.push(Next {
            middleware: middleware,
            next: box None
        });
    }
}

/// An SMTP server, with no commands by default.
pub struct Server<CT> {
    hostname: String,
    max_recipients: uint,
    max_message_size: uint,
    max_command_line_size: uint,
    max_text_line_size: uint,
    commands: Arc<Vec<Command<CT, TcpStream>>>,
    container: CT
}

/// An error that occures when a server starts up
pub enum ServerError {
    /// The hostname could not be retrieved from the system
    Hostname,
    /// Could not bind the socket
    Bind,
    /// Could not listen on the socket
    Listen
}

/// Tells whether an error occured during server setup.
pub type ServerResult<T> = Result<T, ServerError>;

// TODO: logging, via a Trait on the container?
// TODO: fatal error handling
// TODO: actual TCP listening and command handling

impl<CT: Send + Clone> Server<CT> {
    /// Creates a new SMTP server.
    ///
    /// The container can be of any type and can be used to get access to a
    /// bunch of things inside your commands, like database connections,
    /// a logger and more.
    pub fn new(container: CT) -> Server<CT> {
        Server {
            hostname: String::new(),
            max_recipients: 100,
            max_message_size: 65536,
            max_command_line_size: 512,
            max_text_line_size: 1000,
            commands: Arc::new(Vec::with_capacity(16)),
            container: container
        }
    }

    fn set_hostname(&mut self, hostname: &str) {
        self.hostname = hostname.into_string();
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
    pub fn add_command(&mut self, command: Command<CT, TcpStream>) {
        // TODO: Is `make_unique` OK here? I think yes, since the server
        // is setup in its own thread and commands are only added before
        // starting the server. This means `make_unique` should never clone
        // the inner data, but instead always return a a reference to the
        // original one. If it didn't, we might add the new command on a
        // different vector from the one that the server has a reference too.
        // Is that right ?
        self.commands.make_unique().push(command);
    }

    fn increase_max_command_line_size(&mut self, bytes: uint) {
        self.max_command_line_size += bytes;
    }

    fn increase_max_text_line_size(&mut self, bytes: uint) {
        self.max_text_line_size += bytes;
    }

    fn get_hostname_from_system(&mut self) -> ServerResult<String> {
        match rust_gethostname() {
            Ok(s) => {
                Ok(s)
            },
            Err(_) => {
                Err(ServerError::Hostname)
            }
        }
    }

    fn get_listener_for_address(&mut self, address: SocketAddr) -> ServerResult<TcpListener> {
        match TcpListener::bind(address) {
            Ok(listener) => Ok(listener),
            Err(_) => Err(ServerError::Bind)
        }
    }

    fn get_acceptor_for_listener(&mut self, listener: TcpListener) -> ServerResult<TcpAcceptor> {
        match listener.listen() {
            Ok(acceptor) => Ok(acceptor),
            Err(_) => Err(ServerError::Listen)
        }
    }

    fn handle_commands(input: &mut InputStream<TcpStream>, output: &mut OutputStream<TcpStream>, container: &mut CT, commands: &[Command<CT, TcpStream>]) {
        loop {
            match input.read_line() {
                Ok(buffer) => {
                    let line = String::from_utf8_lossy(buffer);
                    println!("line: {}", line);
                    for command in commands.iter() {
                        println!("{}", command.start);
                    }
                },
                Err(err) => {
                    panic!("{}", err);
                }
            }
            // check if it exists
                // yes: do each middleware
                // no: not implemented error message
        }
    }

    fn handle_connection(&self, stream: IoResult<TcpStream>) {
        let mut container = self.container.clone();
        let commands = self.commands.clone();
        spawn(proc() {
            let stream = stream.unwrap();
            let mut input = InputStream::new(stream.clone(), 1000, false);
            let mut output = OutputStream::new(stream.clone(), false);

            Server::<CT>::handle_commands(
                &mut input,
                &mut output,
                &mut container,
                (*commands.deref()).as_slice()
            );
        });
    }

    /// Start the SMTP server on the given address and port.
    pub fn listen(&mut self, ip: IpAddr, port: Port) -> ServerResult<()> {
        if self.hostname.len() == 0 {
            self.hostname = try!(self.get_hostname_from_system());
        }

        let address = SocketAddr {
            ip: ip,
            port: port
        };

        let listener = try!(self.get_listener_for_address(address));

        let mut acceptor = try!(self.get_acceptor_for_listener(listener));

        println!("Server '{}'' listening on {}...", self.hostname, address);

        for conn in acceptor.incoming() {
            self.handle_connection(conn);
        }

        Ok(())
    }
}
