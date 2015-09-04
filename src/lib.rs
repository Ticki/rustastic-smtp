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

//! Rustastic SMTP is meant to provide SMTP tools such as email address parsing
//! utilities as well as a configurable SMTP server and client.
//!
//! The goal is to eventually comply with the
//! [SMTP spec from RFC 5321](http://tools.ietf.org/html/rfc5321).
//!
//! # Example
//!
//! ```no_run
//! #![feature(ip_addr)]
//!
//! extern crate rsmtp;
//!
//! use std::net::{IpAddr, Ipv4Addr};
//! use rsmtp::server::Server;
//! use rsmtp::server::commands::HeloSeen;
//! use rsmtp::server::commands::HeloHandler;
//! use rsmtp::server::commands::helo::get as get_helo_command;
//!
//! #[derive(Clone)]
//! struct Container {
//!     helo_seen: bool
//! }
//!
//! impl Container {
//!     fn new() -> Container {
//!         Container {
//!             helo_seen: false
//!         }
//!     }
//! }
//!
//! impl HeloSeen for Container {
//!     fn helo_seen(&mut self) -> bool {
//!         self.helo_seen
//!     }
//!
//!     fn set_helo_seen(&mut self, helo_seen: bool) {
//!         self.helo_seen = helo_seen;
//!     }
//! }
//!
//! impl HeloHandler for Container {
//!     fn handle_domain(&mut self, domain: &str) -> Result<(), ()> {
//!         println!("Got a client from domain: {:?}", domain);
//!         Ok(())
//!     }
//! }
//!
//! fn main() {
//!     let container = Container::new();
//!     let mut server = Server::new(container);
//!
//!     // Just one command for the example, but you can add more.
//!     // Look in `rsmtp::server::commands` for more commands.
//!     server.add_command(get_helo_command());
//!
//!     // Hypothetical extension support.
//!     server.add_extension("STARTTLS");
//!     server.add_extension("BDAT");
//!
//!     if let Err(_) = server.listen(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 2525) {
//!         println!("Error.");
//!     }
//! }
//! ```

#![deny(unused_qualifications, non_upper_case_globals, missing_docs)]
// #![deny(unused_results)]
#![feature(ip_addr, libc, convert, str_char, std_misc, owned_ascii_ext)]

pub mod client;
pub mod common;
pub mod server;
