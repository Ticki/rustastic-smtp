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

//! The `server` module contains things needed to build an SMTP server, but useless for
//! an SMTP client.

use std::io::net::tcp::{TcpListener, TcpAcceptor, TcpStream};
use std::io::net::ip::{IpAddr};
use std::io::{Listener, Acceptor, IoError, Reader, Writer, InvalidInput};
use super::common::stream::{SmtpReader, SmtpWriter};
use std::sync::Arc;
use std::ascii::OwnedAsciiExt;
use super::common::transaction::{SmtpTransactionState, Init, Helo, Mail, Rcpt, Data};
use super::common::mailbox::Mailbox;
use super::common::{
    MIN_ALLOWED_MESSAGE_SIZE,
    MIN_ALLOWED_LINE_SIZE,
    MIN_ALLOWED_RECIPIENTS
};
use super::common::utils;

// TODO: make SMTP handlers registerable by the library user so we can easily
// add commands and make the server extendable.
struct SmtpHandler<S: Writer+Reader, E: SmtpServerEventHandler> {
    pub command_start: String,
    pub allowed_states: Vec<SmtpTransactionState>,
    pub callback: fn(&mut SmtpReader<S>, &mut SmtpWriter<S>, &mut SmtpTransactionState, &SmtpServerConfig, &mut E, &str) -> Result<(), ()>
}

impl<S: Writer+Reader, E: SmtpServerEventHandler> SmtpHandler<S, E> {
    fn new(command_start: &str, allowed_states: &[SmtpTransactionState], callback: fn(&mut SmtpReader<S>, &mut SmtpWriter<S>, &mut SmtpTransactionState, &SmtpServerConfig, &mut E, &str) -> Result<(), ()>) -> SmtpHandler<S, E> {
        SmtpHandler {
            command_start: command_start.into_string(),
            allowed_states: allowed_states.to_vec(),
            callback: callback
        }
    }
}

/// Hooks into different places of the SMTP server to allow its customization.
///
/// The implementor of this trait you pass to your server is cloned for each
/// new client, which means that you can safely make it have its own fields.
pub trait SmtpServerEventHandler {
    /// Called when a client connects.
    ///
    /// This could be used to check if the sender comes from a banned server,
    /// to log the server information or anything else you desire.
    ///
    /// If `Err(())` is returned, the connection is aborted.
    #[allow(unused_variables)]
    fn handle_connection(&mut self, client_ip: &IpAddr) -> Result<(), ()> {
        Ok(())
    }

    /// Called when we know the domain the client identifies itself with.
    ///
    /// If `Err(())` is returned, the connection is aborted.
    #[allow(unused_variables)]
    fn handle_domain(&mut self, domain: &str) -> Result<(), ()> {
        Ok(())
    }

    /// Called after getting a MAIL command with a sender address.
    ///
    /// The sender address is either `Some(Mailbox)` or `None`. If it is `None`,
    /// it means that the reverse-path (as described in RFC 5321) was null,
    /// which can happen when an email server sends a delivery failure
    /// notification.
    ///
    /// If `Ok(())` is returned, a 250 response is sent. If `Err(())` is returned, a 550 response
    /// is sent and the sender is discarded.
    #[allow(unused_variables)]
    fn handle_sender_address(&mut self, mailbox: Option<&Mailbox>) -> Result<(), ()> {
        Ok(())
    }

    /// Called after getting a RCPT command.
    ///
    /// If `Ok(())` is returned, a 250 response is sent. If `Err(())` is returned, a 550 response
    /// is sent and the recipient is discarded.
    #[allow(unused_variables)]
    fn handle_receiver_address(&mut self, mailbox: &Mailbox) -> Result<(), ()> {
        Ok(())
    }

    /// Called when we know the first body part is coming, ie. when we get the
    /// DATA or BDAT command from the client.
    ///
    /// This could be used to initiate a connection to an HTTP API if that's
    /// where you want to send the body.
    ///
    /// If `Err(())` is returned, the connection is aborted.
    #[allow(unused_variables)]
    fn handle_body_start(&mut self) -> Result<(), ()> {
        Ok(())
    }

    /// Called after getting a part of the body.
    ///
    /// This can happen in several cases:
    ///   * when reading a line of input after a DATA command
    ///   * when getting a chunck of input after a BDAT command
    ///
    /// This can be used to parse the body on the fly or push it to an HTTP
    /// API or whatever you wish to do.
    ///
    /// If `Err(())` is returned, the connection is aborted.
    #[allow(unused_variables)]
    fn handle_body_part(&mut self, part: &[u8]) -> Result<(), ()> {
        Ok(())
    }

    /// Called after getting the last body part.
    ///
    /// If you are sending body parts to an HTTP API, this method could be used
    /// to close the HTTP client.
    ///
    /// If `Err(())` is returned, the connection is aborted.
    #[allow(unused_variables)]
    fn handle_body_end(&mut self) -> Result<(), ()> {
        Ok(())
    }
}

/// Represents the configuration of an SMTP server.
pub struct SmtpServerConfig {
    /// Port on which to listen for incoming messages.
    pub port: u16,
    /// If `true`, debug messages will be printed to the console during transactions.
    pub debug: bool,
    /// The IP on which to `bind (2)` the `TcpListener`.
    pub ip: &'static str,
    /// The domain name used to identify the SMTP server.
    pub domain: &'static str,
    /// The maximum message size, including headers and ending sequence.
    pub max_message_size: uint,
    /// The maximum line size, including `<CRLF>`. At least 1000 per RFC 5321.
    pub max_line_size: uint,
    /// Maximum number of recipients per SMTP transaction.
    pub max_recipients: uint,
    //pub timeout: uint, // at least 5 minutes
    //pub max_clients: uint, // maximum clients to handle at any given time
    //pub max_pending_clients: uint, // maximum clients to put on hold while handling other clients
}

/// Represents an SMTP server which handles client transactions with any kind of stream.
///
/// This is useful for testing purposes as we can test the server from a plain text file. It
/// should not be used for other purposes directly. Use `SmtpServer` instead.
pub struct SmtpServer<S: 'static + Writer + Reader, A: Acceptor<S>, E: 'static + SmtpServerEventHandler> {
    // Underlying acceptor that allows accepting client connections to handle them.
    acceptor: A,
    // Since the config is immutable, we can safely put it in an Arc to avoid
    // re-allocation for every client.
    config: Arc<SmtpServerConfig>,
    // The event handler is not an Arc. This is because we may want to store things
    // inside it that belong to a specific connection.
    event_handler: E,
    // Since the handler are function pointers, these are immutable and can safely
    // be stored in an Arc.
    handlers: Arc<Vec<SmtpHandler<S, E>>>
}

/// Represents an error during creation of an SMTP server.
#[deriving(Show)]
pub enum SmtpServerError {
    /// The system call `bind` failed.
    BindFailed(IoError),
    /// The system call `listen` failed.
    ListenFailed(IoError),
    /// The max message size set in the config is too low.
    MaxMessageSizeTooLow(uint),
    /// The max line size set in the config is too low.
    MaxLineSizeTooLow(uint),
    /// The max number of recipients set in the config is too low.
    MaxRecipientsTooLow(uint)
}

#[test]
fn test_smtp_server_error() {
    // fail!();
}

impl<S: Writer + Reader + Send, A: Acceptor<S>, E: SmtpServerEventHandler+Clone+Send> SmtpServer<S, A, E> {
    /// Creates a new SMTP server from an `Acceptor` implementor. Useful for testing.
    fn new_from_acceptor(acceptor: A, config: SmtpServerConfig, event_handler: E) -> Result<SmtpServer<S, A, E>, SmtpServerError> {
        if config.max_message_size < MIN_ALLOWED_MESSAGE_SIZE {
            Err(MaxMessageSizeTooLow(config.max_message_size))
        } else if config.max_line_size < MIN_ALLOWED_LINE_SIZE {
            Err(MaxLineSizeTooLow(config.max_line_size))
        } else if config.max_recipients < MIN_ALLOWED_RECIPIENTS {
            Err(MaxRecipientsTooLow(config.max_recipients))
        } else {
            Ok(SmtpServer {
                acceptor: acceptor,
                config: Arc::new(config),
                event_handler: event_handler,
                handlers: Arc::new(vec!(
                    SmtpHandler::new("HELO ", &[Init], handle_command_helo),
                    SmtpHandler::new("EHLO ", &[Init], handle_command_helo),
                    SmtpHandler::new("MAIL FROM:", &[Helo], handle_command_mail),
                    SmtpHandler::new("RCPT TO:", &[Mail, Rcpt], handle_command_rcpt),
                    SmtpHandler::new("DATA", &[Rcpt], handle_command_data),
                    SmtpHandler::new("RSET", &[Init, Helo, Mail, Rcpt, Data], handle_command_rset),
                    SmtpHandler::new("VRFY ", &[Init, Helo, Mail, Rcpt, Data], handle_command_vrfy),
                    SmtpHandler::new("EXPN ", &[Init, Helo, Mail, Rcpt, Data], handle_command_expn),
                    SmtpHandler::new("HELP", &[Init, Helo, Mail, Rcpt, Data], handle_command_help),
                    SmtpHandler::new("NOOP", &[Init, Helo, Mail, Rcpt, Data], handle_command_noop),
                    SmtpHandler::new("QUIT", &[Init, Helo, Mail, Rcpt, Data], handle_command_quit)
                ))
            })
        }

    }
}

impl<E: SmtpServerEventHandler + Clone + Send> SmtpServer<TcpStream, TcpAcceptor, E> {
    /// Creates a new SMTP server that listens on `0.0.0.0:2525`.
    pub fn new(config: SmtpServerConfig, event_handler: E) -> Result<SmtpServer<TcpStream, TcpAcceptor, E>, SmtpServerError> {
        match TcpListener::bind(config.ip, config.port) {
            Ok(listener) => {
                if config.debug {
                    println!("rsmtp: info: binding on ip {}", config.ip);
                }
                match listener.listen() {
                    Ok(acceptor) => {
                        if config.debug {
                            println!("rsmtp: info: listening on port {}", config.port);
                        }
                        SmtpServer::new_from_acceptor(acceptor, config, event_handler)
                    },
                    Err(err) => Err(ListenFailed(err))
                }
            },
            Err(err) => Err(BindFailed(err))
        }
    }

    /// Run the SMTP server.
    pub fn run(&mut self) {
        for mut stream_res in self.acceptor.incoming() {
            match stream_res {
                Ok(stream) => {
                    let mut stream = stream.clone();
                    let config = self.config.clone();
                    let mut event_handler = self.event_handler.clone();
                    let handlers = self.handlers.clone();

                    spawn(proc() {
                        SmtpServer::handle_client(
                            &mut stream,
                            config,
                            &mut event_handler,
                            handlers
                        );
                    })
                },
                // Ignore accept error. Is this right? If you think not, please open an issue on Github.
                _ => {}
            }
        }
    }

    // Handle one client inside a separate thread
    fn handle_client(
            stream: &mut TcpStream,
            config: Arc<SmtpServerConfig>,
            event_handler: &mut E,
            handlers: Arc<Vec<SmtpHandler<TcpStream, E>>>) {
        // TODO: remove unwrap and handle error
        event_handler.handle_connection(&stream.peer_name().unwrap().ip).unwrap();

        let mut input = SmtpReader::new(stream.clone(), config.max_line_size, config.debug);
        let mut output = SmtpWriter::new(stream.clone(), config.debug);

        // TODO: WAIT FOR: https://github.com/rust-lang/rust/issues/15802
        //stream.stream.set_deadline(local_config.timeout);

        // Send the opening welcome message.
        output.write_line(format!("220 {}", config.domain).as_slice()).unwrap();

        // Loop over incoming commands and process them.
        SmtpServer::inner_loop(
            &mut input,
            &mut output,
            config,
            event_handler,
            handlers
        );
    }

    // Get the right handler for a given command line.
    fn get_handler_for_line<'a>(
            handlers: &'a [SmtpHandler<TcpStream, E>],
            line: &str) -> Option<&'a SmtpHandler<TcpStream, E>> {
        for h in handlers.iter() {
            // Don't check lines shorter than required. This also avoids getting an
            // out of bounds error below.
            if line.len() < h.command_start.len() {
                continue;
            }
            let line_start = line.as_slice().slice_to(h.command_start.len())
                .into_string().into_ascii_upper();
            // Check that the begining of the command matches an existing SMTP
            // command. This could be something like "HELO " or "RCPT TO:".
            if line_start.as_slice().starts_with(h.command_start.as_slice()) {
                return Some(h);
            }
        }
        None
    }

    fn get_line_and_handler<'a>(
            input: &mut SmtpReader<TcpStream>,
            handlers: &'a [SmtpHandler<TcpStream, E>]) -> Result<(String, Option<&'a SmtpHandler<TcpStream, E>>), IoError> {
        match input.read_line() {
            Ok(bytes) => {
                let line = String::from_utf8_lossy(bytes.as_slice()).into_string();
                let handler = SmtpServer::get_handler_for_line(handlers, line.as_slice());

                Ok((line, handler))
            },
            Err(err) => {
                Err(err)
            }
        }
    }

    fn read_and_handle_command(
            input: &mut SmtpReader<TcpStream>,
            output: &mut SmtpWriter<TcpStream>,
            handlers: &[SmtpHandler<TcpStream, E>],
            state: &mut SmtpTransactionState,
            config: &SmtpServerConfig,
            event_handler: &mut E) -> Result<(), ()> {
        match SmtpServer::get_line_and_handler(input, handlers) {
            Ok((line, Some(handler))) => {
                if handler.allowed_states.contains(state) {
                    let rest = line.as_slice().slice_from(handler.command_start.len());
                    (handler.callback)(
                        input,
                        output,
                        state,
                        config,
                        event_handler,
                        rest
                    )
                } else {
                    output.write_line("503 Bad sequence of commands").unwrap();
                    Ok(())
                }
            },
            Ok((_, None)) => {
                output.write_line("500 Command unrecognized").unwrap();
                Ok(())
            },
            Err(err) => {
                // If the line was too long, notify the client.
                match err.kind {
                    InvalidInput => {
                        // TODO: check error desc to make sure this is right
                        output.write_line("500 Command line too long, max is 512 bytes").unwrap();
                        Ok(())
                    },
                    _ => {
                        // If we get here, the error is unexpected. What to do with it?
                        Err(())
                    }
                }
            }
        }
    }

    // Forever, looooop over command lines and handle them.
    fn inner_loop(
            input: &mut SmtpReader<TcpStream>,
            output: &mut SmtpWriter<TcpStream>,
            config: Arc<SmtpServerConfig>,
            event_handler: &mut E,
            handlers: Arc<Vec<SmtpHandler<TcpStream, E>>>) {
        // Setup the initial transaction state for this client.
        let mut state = Init;

        // Read and handle commands.
        'main_loop: loop {
            SmtpServer::read_and_handle_command(
                input,
                output,
                handlers.as_slice(),
                &mut state,
                config.deref(),
                event_handler
            ).unwrap();
        }
    }
}

#[test]
fn test_smtp_server_new() {
    // fail!();
}

#[test]
fn test_smtp_server_new_from_acceptor() {
    // fail!();
}

#[test]
fn test_smtp_server_handlers() {
    // fail!();
}

#[test]
fn test_smtp_server_run() {
    // fail!();
}

#[allow(unused_variables)]
fn handle_command_helo<S: Writer+Reader, E: SmtpServerEventHandler>(
                        input: &mut SmtpReader<S>,
                        output: &mut SmtpWriter<S>,
                        state: &mut SmtpTransactionState,
                        config: &SmtpServerConfig,
                        event_handler: &mut E,
                        line: &str) -> Result<(), ()> {
    if line.len() == 0 {
        output.write_line("501 Domain name not provided").unwrap();
    } else if utils::get_domain_len(line) != line.len() {
        output.write_line("501 Domain name is invalid").unwrap();
    } else {
        match event_handler.handle_domain(line) {
            Ok(_) => {
                *state = Helo;
                output.write_line("250 OK").unwrap();
            },
            Err(_) => {
                output.write_line("550 Domain not taken").unwrap();
            }
        }
    }
    Ok(())
}

#[test]
fn test_command_helo() {
    // fail!();
}

#[allow(unused_variables)]
fn handle_command_mail<S: Writer+Reader, E: SmtpServerEventHandler>(
                        input: &mut SmtpReader<S>,
                        output: &mut SmtpWriter<S>,
                        state: &mut SmtpTransactionState,
                        config: &SmtpServerConfig,
                        event_handler: &mut E,
                        line: &str) -> Result<(), ()> {
    if line.len() < 2 || line.char_at(0) != '<' || line.char_at(line.len() - 1) != '>' {
        output.write_line("501 Email address invalid, must start with < and end with >").unwrap();
    } else if line == "<>" {
        match event_handler.handle_sender_address(None) {
            Ok(_) => {
                *state = Mail;
                output.write_line("250 OK").unwrap();
            },
            Err(_) => {
                output.write_line("550 Mailnot available").unwrap();
            }
        }
    } else {
        let mailbox_res = Mailbox::parse(line.slice(1, line.len() - 1));
        match mailbox_res {
            Err(err) => {
                output.write_line(format!("553 Email address invalid: {}", err).as_slice()).unwrap();
            },
            Ok(mailbox) => {
                match event_handler.handle_sender_address(Some(&mailbox)) {
                    Ok(_) => {
                        *state = Mail;
                        output.write_line("250 OK").unwrap();
                    },
                    Err(_) => {
                        output.write_line("550 Mailnot taken").unwrap();
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn test_command_mail() {
    // fail!();
}

#[allow(unused_variables)]
fn handle_command_rcpt<S: Writer+Reader, E: SmtpServerEventHandler>(
                        input: &mut SmtpReader<S>,
                        output: &mut SmtpWriter<S>,
                        state: &mut SmtpTransactionState,
                        config: &SmtpServerConfig,
                        event_handler: &mut E,
                        line: &str) -> Result<(), ()> {
    // TODO: check maximum number of recipients? Maybe after the event handler
    // sends back `Ok(())`?
    if false {
        output.write_line("452 Too many recipients").unwrap();
    } else if line.char_at(0) != '<' || line.char_at(line.len() - 1) != '>' {
        output.write_line("501 Email address invalid, must start with < and end with >").unwrap();
    } else {
        let mailbox_res = Mailbox::parse(line.slice(1, line.len() - 1));
        match mailbox_res {
            Err(err) => {
                output.write_line(format!("553 Email address invalid: {}", err).as_slice()).unwrap();
            },
            Ok(mailbox) => {
                match event_handler.handle_receiver_address(&mailbox) {
                    Ok(_) => {
                        *state = Rcpt;
                        output.write_line("250 OK").unwrap();
                    },
                    Err(_) => {
                        output.write_line("550 Mailnot available").unwrap();
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn test_command_rcpt() {
    // fail!();
}

#[allow(unused_variables)]
fn handle_command_data<S: Writer+Reader, E: SmtpServerEventHandler>(
                        input: &mut SmtpReader<S>,
                        output: &mut SmtpWriter<S>,
                        state: &mut SmtpTransactionState,
                        config: &SmtpServerConfig,
                        event_handler: &mut E,
                        line: &str) -> Result<(), ()> {
    if line.len() != 0 {
        output.write_line("501 No arguments allowed").unwrap();
    } else {
        output.write_line("354 Start mail input; end with <CRLF>.<CRLF>").unwrap();

        // Inform our event handler that mail data is about to be received.
        event_handler.handle_body_start().unwrap();

        let mut size = 0;
        loop {
            match input.read_line() {
                Ok(data_line) => {
                    // Here, we check that we have already got some data, which
                    // means that we have read a line, which means we have just
                    // seen `<CRLF>`. And then, we check if the current line
                    // which we know to end with `<CRLF>` as well contains a
                    // single dot.
                    // All in all, this means we check for `<CRLF>.<CRLF>`.
                    if size != 0 && data_line == &['.' as u8] {
                        break;
                    }
                    // TODO: support transparency. Here or in the reader ?

                    event_handler.handle_body_part(data_line).unwrap();

                    size += data_line.len();

                    if size > config.max_message_size {
                        // TODO: add an error handler in the event handler?
                        output.write_line(format!(
                            "552 Too much mail data, max {} bytes",
                            config.max_message_size
                        ).as_slice()).unwrap();
                        return Ok(());
                    }
                },
                Err(err) => {
                    return Err(());
                }
            }
        }

        // Inform our event handler that all data has been received.
        event_handler.handle_body_end().unwrap();

        // We're all good !
        state.reset();
        output.write_line("250 OK").unwrap();
    }
    Ok(())
}

#[test]
fn test_command_data() {
    // fail!();
}

#[allow(unused_variables)]
fn handle_command_rset<S: Writer+Reader, E: SmtpServerEventHandler>(
                        input: &mut SmtpReader<S>,
                        output: &mut SmtpWriter<S>,
                        state: &mut SmtpTransactionState,
                        config: &SmtpServerConfig,
                        event_handler: &mut E,
                        line: &str) -> Result<(), ()> {
    if line.len() != 0 {
        output.write_line("501 No arguments allowed").unwrap();
    } else {
        state.reset();
        output.write_line("250 OK").unwrap();
    }
    Ok(())
}

#[test]
fn test_command_rset() {
    // fail!();
}

#[allow(unused_variables)]
fn handle_command_vrfy<S: Writer+Reader, E: SmtpServerEventHandler>(
                        input: &mut SmtpReader<S>,
                        output: &mut SmtpWriter<S>,
                        state: &mut SmtpTransactionState,
                        config: &SmtpServerConfig,
                        event_handler: &mut E,
                        line: &str) -> Result<(), ()> {
    output.write_line("252 Cannot VRFY user").unwrap();
    Ok(())
}

#[test]
fn test_command_vrfy() {
    // fail!();
}

#[allow(unused_variables)]
fn handle_command_expn<S: Writer+Reader, E: SmtpServerEventHandler>(
                        input: &mut SmtpReader<S>,
                        output: &mut SmtpWriter<S>,
                        state: &mut SmtpTransactionState,
                        config: &SmtpServerConfig,
                        event_handler: &mut E,
                        line: &str) -> Result<(), ()> {
    output.write_line("252 Cannot EXPN mailing list").unwrap();
    Ok(())
}

#[test]
fn test_command_expn() {
    // fail!();
}

#[allow(unused_variables)]
fn handle_command_help<S: Writer+Reader, E: SmtpServerEventHandler>(
                        input: &mut SmtpReader<S>,
                        output: &mut SmtpWriter<S>,
                        state: &mut SmtpTransactionState,
                        config: &SmtpServerConfig,
                        event_handler: &mut E,
                        line: &str) -> Result<(), ()> {
    if line.len() == 0 || line.char_at(0) == ' ' {
        output.write_line("502 Command not implemented").unwrap();
    } else {
        output.write_line("500 Command unrecognized").unwrap();
    }
    Ok(())
}

#[test]
fn test_command_help() {
    // fail!();
}

#[allow(unused_variables)]
fn handle_command_noop<S: Writer+Reader, E: SmtpServerEventHandler>(
                        input: &mut SmtpReader<S>,
                        output: &mut SmtpWriter<S>,
                        state: &mut SmtpTransactionState,
                        config: &SmtpServerConfig,
                        event_handler: &mut E,
                        line: &str) -> Result<(), ()> {
    if line.len() == 0 || line.char_at(0) == ' ' {
        output.write_line("250 OK").unwrap();
    } else {
        output.write_line("500 Command unrecognized").unwrap();
    }
    Ok(())
}

#[test]
fn test_command_noop() {
    // fail!();
}

#[allow(unused_variables)]
fn handle_command_quit<S: Writer+Reader, E: SmtpServerEventHandler>(
                        input: &mut SmtpReader<S>,
                        output: &mut SmtpWriter<S>,
                        state: &mut SmtpTransactionState,
                        config: &SmtpServerConfig,
                        event_handler: &mut E,
                        line: &str) -> Result<(), ()> {
    output.write_line(format!("221 {}", config.domain).as_slice()).unwrap();
    Err(())
}

#[test]
fn test_command_quit() {
    // fail!();
}
