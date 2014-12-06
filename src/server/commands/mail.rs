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

use std::io::net::tcp::TcpStream;
use super::super::super::common::stream::{InputStream, OutputStream};
use super::super::{Next, Command};

/// Holds the state needed by the MAIL command
#[deriving(Clone)]
pub struct MailFromState {
    ehlo: bool
}

impl MailFromState {
    /// Create an initial state for the MAIL command
    pub fn new() -> MailFromState {
        MailFromState {
            ehlo: false
        }
    }
}

/// Methods needed by the mail command to read the current state
pub trait MailFrom {
    /// Returns the mutable state
    fn mail_from_state_mut(&mut self) -> &mut MailFromState;

    /// Returns the immutable state
    fn mail_from_state(&mut self) -> &MailFromState {
        &*self.mail_from_state_mut()
    }
}

fn check_ehlo<CT: MailFrom>(container: &mut CT, input: &mut InputStream<TcpStream>, output: &mut OutputStream<TcpStream>, line: &str, next: Option<Next<CT, TcpStream>>) {
    if !container.mail_from_state().ehlo {
        output.write_line("503 Bad sequence of commands").unwrap();
    }

    next.unwrap().call(container, input, output, line);
}

fn say_ok<CT>(container: &mut CT, input: &mut InputStream<TcpStream>, output: &mut OutputStream<TcpStream>, line: &str, next: Option<Next<CT, TcpStream>>) {
    output.write_line("250 OK").unwrap();
}

/// Returns the MAIL command
pub fn get<CT: MailFrom + Clone + Send>() -> Command<CT, TcpStream> {
    let mut command = Command::new();
    command.starts_with("MAIL FROM:");
    command.middleware(check_ehlo);
    command.middleware(say_ok);

    command
}
