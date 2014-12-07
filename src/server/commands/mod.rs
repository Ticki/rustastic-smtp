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

use std::collections::HashMap;

/// The MAIL command.
pub mod mail;

/// Holds state of different types for server commands.
#[deriving(Clone)]
pub struct State {
    bool_state: HashMap<String, bool>,
    int_state: HashMap<String, int>,
    uint_state: HashMap<String, uint>,
    string_state: HashMap<String, String>
}

impl State {
    /// Creates a new state object with no initial state.
    pub fn new() -> State {
        State {
            bool_state: HashMap::new(),
            int_state: HashMap::new(),
            uint_state: HashMap::new(),
            string_state: HashMap::new()
        }
    }

    /// Get a boolean value.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.bool_state.get(key).map(|v| *v)
    }

    /// Get a boolean value or its default.
    pub fn get_bool_default(&self, key: &str, default: bool) -> bool {
        self.bool_state.get(key).map_or(default, |v| *v)
    }

    /// Sets a boolean value.
    #[allow(unused_results)]
    pub fn set_bool(&mut self, key: &str, value: bool) {
        self.bool_state.insert(key.into_string(), value);
    }

    /// Get an integer.
    pub fn get_int(&self, key: &str) -> Option<int> {
        self.int_state.get(key).map(|v| *v)
    }

    /// Get an integer or its default.
    pub fn get_int_default(&self, key: &str, default: int) -> int {
        self.int_state.get(key).map_or(default, |v| *v)
    }

    /// Sets an integer.
    #[allow(unused_results)]
    pub fn set_int(&mut self, key: &str, value: int) {
        self.int_state.insert(key.into_string(), value);
    }

    /// Get an unsigned integer.
    pub fn get_uint(&self, key: &str) -> Option<uint> {
        self.uint_state.get(key).map(|v| *v)
    }

    /// Get an unsigned integer or its default.
    pub fn get_uint_default(&self, key: &str, default: uint) -> uint {
        self.uint_state.get(key).map_or(default, |v| *v)
    }

    /// Sets an unsigned integer.
    #[allow(unused_results)]
    pub fn set_uint(&mut self, key: &str, value: uint) {
        self.uint_state.insert(key.into_string(), value);
    }

    /// Get a string.
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.string_state.get(key).map(|v| (*v).clone())
    }

    /// Get a string or its default.
    pub fn get_string_default(&self, key: &str, default: &str) -> String {
        self.string_state.get(key).map_or(default.into_string(), |v| (*v).clone())
    }

    /// Sets a string.
    #[allow(unused_results)]
    pub fn set_string(&mut self, key: &str, value: String) {
        self.string_state.insert(key.into_string(), value);
    }

    /// Resets the state.
    pub fn reset(&mut self) {
        self.bool_state.clear();
        self.int_state.clear();
        self.uint_state.clear();
        self.string_state.clear();
    }
}

/// Allows commands to get access to information about the state of the
/// current transaction.
pub trait Stateful {
    /// Returns the state object for the current connection.
    fn state(&mut self) -> &mut State;
}
