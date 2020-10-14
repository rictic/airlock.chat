extern crate vergen;

use vergen::{generate_cargo_keys, ConstantsFlags};

fn main() {
  // Setup the flags, toggling just the 'SHA' flag
  let mut flags = ConstantsFlags::empty();
  flags.toggle(ConstantsFlags::SHA);

  // Generate the 'cargo:' key output
  generate_cargo_keys(flags).expect("Unable to generate the cargo keys!");
}
