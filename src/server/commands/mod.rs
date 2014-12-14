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

/// The MAIL command.
pub mod mail;

/// The HELO & EHLO commands.
pub mod helo;

/// Allows commands to get access to information about the state of the
/// current transaction.
pub trait HeloSeen {
    /// Returns the state object for the current connection.
    fn helo_seen(&mut self) -> bool;

    /// Sets if we have HELO or not.
    fn set_helo_seen(&mut self, helo_seen: bool);
}
