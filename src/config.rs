// SPDX-License-Identifier: MIT

use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};

#[derive(Debug, Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 1]
pub struct Config {
    pub on_script: String,
    pub off_script: String,
    pub status_script: String,
    pub status_check_interval: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            on_script: String::new(),
            off_script: String::new(),
            status_script: String::new(),
            status_check_interval: 10,
        }
    }
}
