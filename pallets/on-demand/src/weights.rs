
//! Autogenerated weights for `pallet_on_demand`
//!
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 43.0.0
//! DATE: 2024-12-24, STEPS: `20`, REPEAT: `50`, LOW RANGE: `[]`, HIGH RANGE: `[]`
//! WORST CASE MAP SIZE: `1000000`
//! HOSTNAME: `sergej-B650-AORUS-ELITE-AX`, CPU: `AMD Ryzen 9 7900X3D 12-Core Processor`
//! WASM-EXECUTION: `Compiled`, CHAIN: `None`, DB CACHE: `1024`

// Executed Command:
// ./target/release/parachain-example-node
// benchmark
// pallet
// --pallet
// pallet_on_demand
// --steps
// 20
// --repeat
// 50
// --output
// ../pallets/on-demand/src/weights.rs
// --template
// ./config/frame-weight-template.hbs
// --extrinsic=*
// --wasm-execution=compiled
// --heap-pages=4096

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use core::marker::PhantomData;

/// Weight functions needed for `pallet_on_demand`.
pub trait WeightInfo {
	fn set_slot_width() -> Weight;
	fn set_threshold_parameter() -> Weight;
	fn set_bulk_mode() -> Weight;
	fn on_reward() -> Weight;
	fn should_place_order() -> Weight;
}

/// Weights for `pallet_on_demand` using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// Storage: `OnDemand::SlotWidth` (r:0 w:1)
	/// Proof: `OnDemand::SlotWidth` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	fn set_slot_width() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 3_306_000 picoseconds.
		Weight::from_parts(3_497_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `OnDemand::ThresholdParameter` (r:0 w:1)
	/// Proof: `OnDemand::ThresholdParameter` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	fn set_threshold_parameter() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 3_326_000 picoseconds.
		Weight::from_parts(3_487_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `OnDemand::ThresholdParameter` (r:0 w:1)
	/// Proof: `OnDemand::ThresholdParameter` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	fn set_bulk_mode() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 3_326_000 picoseconds.
		Weight::from_parts(3_487_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `OnDemand::ThresholdParameter` (r:0 w:1)
	/// Proof: `OnDemand::ThresholdParameter` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	fn on_reward() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 3_326_000 picoseconds.
		Weight::from_parts(3_487_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `OnDemand::ThresholdParameter` (r:0 w:1)
	/// Proof: `OnDemand::ThresholdParameter` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	fn should_place_order() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 3_326_000 picoseconds.
		Weight::from_parts(3_487_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
}

// For backwards compatibility and tests.
impl WeightInfo for () {
	/// Storage: `OnDemand::SlotWidth` (r:0 w:1)
	/// Proof: `OnDemand::SlotWidth` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	fn set_slot_width() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 3_306_000 picoseconds.
		Weight::from_parts(3_497_000, 0)
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	/// Storage: `OnDemand::ThresholdParameter` (r:0 w:1)
	/// Proof: `OnDemand::ThresholdParameter` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	fn set_threshold_parameter() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 3_326_000 picoseconds.
		Weight::from_parts(3_487_000, 0)
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	/// Storage: `OnDemand::ThresholdParameter` (r:0 w:1)
	/// Proof: `OnDemand::ThresholdParameter` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	fn set_bulk_mode() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 3_326_000 picoseconds.
		Weight::from_parts(3_487_000, 0)
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	/// Storage: `OnDemand::ThresholdParameter` (r:0 w:1)
	/// Proof: `OnDemand::ThresholdParameter` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	fn on_reward() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 3_326_000 picoseconds.
		Weight::from_parts(3_487_000, 0)
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
	/// Storage: `OnDemand::ThresholdParameter` (r:0 w:1)
	/// Proof: `OnDemand::ThresholdParameter` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	fn should_place_order() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 3_326_000 picoseconds.
		Weight::from_parts(3_487_000, 0)
			.saturating_add(RocksDbWeight::get().writes(1_u64))
	}
}
