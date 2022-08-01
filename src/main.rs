use anyhow::{anyhow, Context, Result};
use csv::Reader;
use std::env;

use crate::engine::process_transactions;

mod engine;
mod model;

fn main() -> Result<()> {
    // For more complex/generic apps, we should use a crate like `clap` for argument handling, but
    // in this case, our app interface is well-defined and consistent + we're prioritizing speed.
    let input_file = env::args()
        .nth(1)
        .ok_or_else(|| anyhow!("Missing input file!"))?;

    let mut csv_reader = Reader::from_path(&input_file)
        .with_context(|| format!("Cannot read input file: {}", input_file))?;

    let transactions = csv_reader
        .deserialize()
        .map(|tx| tx.map_err(|error| error.into()));

    let client_states = process_transactions(transactions)
        .with_context(|| format!("Cannot process input file: {}", input_file))?;

    Ok(())
}
