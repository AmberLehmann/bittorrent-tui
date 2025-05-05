use clap::Parser;

// TODO: Parse args with clap
/// Rusty implementation of the Chord protocol.
#[derive(Debug, Parser)]
#[command(about, arg_required_else_help = true, long_about = None)]
pub struct Args {
    pub tui: bool,
}
