#![feature(portable_simd)]
#![feature(iter_array_chunks)]

pub mod fpga_layout;
pub mod netlist;
pub mod placer;

pub use fpga_layout::*;
pub use netlist::*;
pub use placer::*;
