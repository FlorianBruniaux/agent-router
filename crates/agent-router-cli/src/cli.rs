// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
use clap::Parser;

use crate::command::Commands;

#[derive(Parser)]
#[command(name = "agent-router", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}
