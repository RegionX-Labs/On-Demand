//! Pallet for managing the on-demand configuration.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use cumulus_primitives_core::relay_chain;
use frame_support::{pallet_prelude::*, traits::tokens::Balance as BalanceT};
use frame_system::pallet_prelude::*;
use sp_runtime::{traits::Header, SaturatedConversion};

pub use pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;

const LOG_TARGET: &str = "pallet-on-demand";

#[cfg(feature = "runtime-benchmarks")]
pub use benchmarking::BenchmarkHelper;

pub trait OnReward<AccountId, Balance, R: RewardSize<Balance>> {
	fn reward(rewardee: AccountId);
}

pub trait RewardSize<Balance> {
	fn reward_size() -> Balance;
}

/// Taken from: https://github.com/paritytech/polkadot-sdk/blob/master/cumulus/pallets/parachain-system/src/lib.rs
///
/// NOTE: Copied here because in the original implementation it is missing `MaxEncodedLen`.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, Default, RuntimeDebug, MaxEncodedLen)]
pub struct RelayChainState {
	/// Current relay chain height.
	pub number: relay_chain::BlockNumber,
	/// State root for current relay chain height.
	pub state_root: relay_chain::Hash,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use crate::weights::WeightInfo;
	use alloc::{boxed::Box, vec::Vec};
	use codec::MaxEncodedLen;
	use cumulus_pallet_parachain_system::{
		relay_state_snapshot::Error as RelayError, RelayChainStateProof, RelaychainStateProvider,
	};
	use frame_support::{
		traits::{
			fungible::{Inspect, Mutate},
			tokens::Balance,
		},
		weights::Weight,
		DefaultNoBound,
	};
	use frame_system::EventRecord;
	use order_primitives::{well_known_keys::EVENTS, OrderInherentData};
	use polkadot_runtime_parachains::on_demand;
	use sp_runtime::{
		traits::{AtLeast32BitUnsigned, Convert},
		AccountId32, RuntimeAppPublic,
	};
	use sp_core::H256;

	/// The module configuration trait.
	#[pallet::config]
	pub trait Config:
		frame_system::Config<AccountId = AccountId32> + pallet_aura::Config + pallet_session::Config
	{
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Relay chain balance type.
		type RelayChainBalance: Balance;

		/// The admin origin for managing the on-demand configuration.
		type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Block number type.
		type BlockNumber: Parameter
			+ Member
			+ Default
			+ MaybeSerializeDeserialize
			+ MaxEncodedLen
			+ AtLeast32BitUnsigned;

		/// Given that we want to keep this pallet as generic as possible, we don't assume the type
		/// of the threshold.
		///
		/// We are adding this for implementations that have some kind of threshold and want it to
		/// be stored within the runtime.
		///
		/// For example, this threshold could represent the total weight of all the ready
		/// transactions from the pool, or their total fees.
		///
		/// NOTE: If there isn't a threshold parameter, this can simply be set to `()`.
		type ThresholdParameter: Member
			+ Parameter
			+ Default
			+ MaybeSerializeDeserialize
			+ MaxEncodedLen;

		/// The currency for rewarding order placers.
		type Currency: Mutate<Self::AccountId>;

		/// Type which implements the logic for rewarding order placers.
		type OnReward: OnReward<
			Self::AccountId,
			<Self::Currency as Inspect<Self::AccountId>>::Balance,
			Self::RewardSize,
		>;

		/// Reward size for order placers.
		type RewardSize: RewardSize<<Self::Currency as Inspect<Self::AccountId>>::Balance>;

		/// Type converting `Self::ValidatorId` to `Self::AccountId`.
		type ToAccountId: Convert<Self::ValidatorId, Self::AccountId>;

		/// Defines how many past relay chain headers will we store in the rutnime.
		type StateHistoryDepth: Get<u32>;

		/// Type for getting the current relay chain state.
		type RelayChainStateProvider: RelaychainStateProvider;

		/// Relay chain header type.
		type RelayChainHeader: Header<Hash = H256, Number = relay_chain::BlockNumber>;

		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: crate::BenchmarkHelper<Self::ThresholdParameter>;

		/// Weight Info
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// The authorities for the current on-demand slot.
	#[pallet::storage]
	#[pallet::getter(fn authorities)]
	pub type Authorities<T: Config> =
		StorageValue<_, BoundedVec<T::AuthorityId, T::MaxAuthorities>, ValueQuery>;

	/// Defines how often a new on-demand order is created, based on the number of slots.
	///
	/// This will limit the block production rate. However, if set to a low value, collators
	/// will struggle to coordinate effectively, leading to unnecessary multiple orders being
	/// placed.
	#[pallet::storage]
	#[pallet::getter(fn slot_width)]
	pub type SlotWidth<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// The threshold parameter stored in the runtime state.
	///
	/// This will determine whether an on-demand order should be placed by a collator.
	#[pallet::storage]
	#[pallet::getter(fn threshold_parameter)]
	pub type ThresholdParameter<T: Config> = StorageValue<_, T::ThresholdParameter, ValueQuery>;

	/// When in bulk mode we skip the `on_initialize` logic of this pallet.c
	#[pallet::storage]
	#[pallet::getter(fn bulk_mode)]
	pub type BulkMode<T: Config> = StorageValue<_, (), OptionQuery>;

	/// Keeps track of the relay chain block number in which the last on-demand order was submitted.
	///
	/// We use this to prove order proof validity. Collator cannot provide proof to an order that
	/// was before or in the same block as the last order.
	#[pallet::storage]
	#[pallet::getter(fn last_order)]
	pub type LastOrder<T: Config> = StorageValue<_, relay_chain::BlockNumber, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Configuration of the coretime chain was set.
		SlotWidthSet { width: u32 },
		/// Threshold parameter set.
		ThresholdParameterSet { parameter: T::ThresholdParameter },
		/// We rewarded the order placer.
		OrderPlacerRewarded { order_placer: T::AccountId },
		/// Bulk mode set.
		BulkModeSet { bulk_mode: bool },
	}

	#[pallet::error]
	#[derive(PartialEq)]
	pub enum Error<T> {
		/// Invalid proof provided for system events key
		InvalidProof,
		/// Too old proof was provided.
		OldProof,
		/// Missing state root in the inherent data.
		MissingStateRoot,
		/// Failed to read the relay chain proof.
		FailedProofReading,
		/// Failed to get the order placer account based on the authority id.
		FailedToGetOrderPlacerAccount,
		/// We failed to decode inherent data.
		FailedToDecodeInherentData,
		/// Missing the block in which the order was made.
		MissingBlock,
	}

	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// Initial threshold parameter.
		pub threshold_parameter: T::ThresholdParameter,
		/// Initial mode.
		pub bulk_mode: bool,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			ThresholdParameter::<T>::set(self.threshold_parameter.clone());
			if self.bulk_mode {
				BulkMode::<T>::set(Some(()));
			}
		}
	}

	// NOTE: always place pallet-on-demand after the aura pallet.
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		// fn on_initialize(_: BlockNumberFor<T>) -> Weight {
		// 	Weight::zero()
		// }

		fn on_finalize(_: BlockNumberFor<T>) {
			// Update to the latest AuRa authorities.
			//
			// By updating the authorities on finalize we will always have the previous set
			// from the previous block used within this pallet.
			Authorities::<T>::put(pallet_aura::Authorities::<T>::get());
		}
	}

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = MakeFatalError<Error<T>>;

		const INHERENT_IDENTIFIER: InherentIdentifier =
			order_primitives::ON_DEMAND_INHERENT_IDENTIFIER;

		fn create_inherent(data: &InherentData) -> Option<Self::Call> {
			log::info!(
				target: LOG_TARGET,
				"Create inherent"
			);

			let data: OrderInherentData<T::RelayChainHeader> = data
				.get_data(&Self::INHERENT_IDENTIFIER)
				.ok()
				.flatten()
				.expect("there is not data to be posted; qed");

			Some(Call::create_order { data })
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::create_order { .. })
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight((0, DispatchClass::Mandatory))]
		pub fn create_order(
			origin: OriginFor<T>,
			data: OrderInherentData<T::RelayChainHeader>,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			if BulkMode::<T>::get().is_some() {
				return Ok(().into());
			}
			if Authorities::<T>::get().len().is_zero() {
				return Ok(().into());
			}

			let Some(block) = data.ancestors.first() else {
				return Err(Error::<T>::MissingBlock.into());
			};

			ensure!(
				*block.number() <= LastOrder::<T>::get() || LastOrder::<T>::get().is_zero(),
				Error::<T>::OldProof
			);

			// Things TO DO:
			// Prove that order was within a relay chain block that is an ancestor of our current
			// relay parent.
			let current_state_root = T::RelaychainDataProvider::current_relay_chain_state();

			for header in data.ancestors.iter() {
				
			}

			// If the last verified parent is the ancestor, it's valid

			// TODO: ensure we were supposed to place an order.

			let relay_state_proof = RelayChainStateProof::new(
				data.para_id,
				*block.state_root(),
				data.relay_storage_proof,
			)
			.expect("Invalid relay chain state proof");

			let events = relay_state_proof
				.read_entry::<Vec<Box<EventRecord<rococo_runtime::RuntimeEvent, T::Hash>>>>(
					EVENTS, None,
				)
				.map_err(|e| match e {
					RelayError::ReadEntry(_) => Error::InvalidProof,
					_ => Error::<T>::FailedProofReading,
				})?;

			let result: Vec<(u128, AccountId32)> = events
				.into_iter()
				.filter_map(|item| match item.event {
					rococo_runtime::RuntimeEvent::OnDemandAssignmentProvider(
						on_demand::Event::OnDemandOrderPlaced { para_id, spot_price, ordered_by },
					) if para_id == data.para_id => Some((spot_price, ordered_by)),
					_ => None,
				})
				.collect();

			let Some(order_placer) = Self::order_placer() else {
				return Ok(().into());
			};
			let order_placer_acc = pallet_session::KeyOwner::<T>::get((
				sp_application_crypto::key_types::AURA,
				order_placer.to_raw_vec(),
			))
			.ok_or(Error::<T>::FailedToGetOrderPlacerAccount)?;

			let Some(order_placer) = result
				.into_iter()
				.find(|(_, ordered_by)| {
					// In most implementations the validator id is same as account id.
					<T as pallet_session::Config>::ValidatorIdOf::convert(ordered_by.clone()) ==
						Some(order_placer_acc.clone())
				})
				.map(|o| o.1)
			else {
				return Ok(().into());
			};

			LastOrder::<T>::set(*block.number());

			T::OnReward::reward(T::ToAccountId::convert(order_placer_acc));

			Ok(().into())
		}

		/// Set the slot width for on-demand blocks.
		///
		/// - `origin`: Must be Root or pass `AdminOrigin`.
		/// - `width`: The slot width in relay chain blocks.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::set_slot_width())]
		pub fn set_slot_width(origin: OriginFor<T>, width: u32) -> DispatchResult {
			T::AdminOrigin::ensure_origin_or_root(origin)?;

			SlotWidth::<T>::set(width.clone());
			Self::deposit_event(Event::SlotWidthSet { width });

			Ok(())
		}

		/// Set the threshold parameter.
		///
		/// - `origin`: Must be Root or pass `AdminOrigin`.
		/// - `parameter`: The threshold parameter.
		#[pallet::call_index(2)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::set_threshold_parameter())]
		pub fn set_threshold_parameter(
			origin: OriginFor<T>,
			parameter: T::ThresholdParameter,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin_or_root(origin)?;

			ThresholdParameter::<T>::set(parameter.clone());
			Self::deposit_event(Event::ThresholdParameterSet { parameter });

			Ok(())
		}

		/// Set the threshold parameter.
		///
		/// - `origin`: Must be Root or pass `AdminOrigin`.
		/// - `bulk_mode`: Defines whether we want to switch to bulk mode.
		#[pallet::call_index(3)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::set_bulk_mode())]
		pub fn set_bulk_mode(origin: OriginFor<T>, bulk_mode: bool) -> DispatchResult {
			T::AdminOrigin::ensure_origin_or_root(origin)?;

			if bulk_mode {
				BulkMode::<T>::set(Some(()));
			} else {
				BulkMode::<T>::kill();
			}

			Self::deposit_event(Event::BulkModeSet { bulk_mode });

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		fn order_placer() -> Option<T::AuthorityId> {
			let slot_width = SlotWidth::<T>::get();
			let para_height = frame_system::Pallet::<T>::block_number();
			let authorities = Authorities::<T>::get();

			let slot: u128 = (para_height >> slot_width).saturated_into();
			let indx = slot % authorities.len() as u128;

			let authority_id = authorities.get(indx as usize).cloned();

			authority_id
		}
	}

	impl<T: Config>
		OnReward<T::AccountId, <T::Currency as Inspect<T::AccountId>>::Balance, T::RewardSize>
		for Pallet<T>
	{
		fn reward(rewardee: T::AccountId) {
			let reward_size = T::RewardSize::reward_size();
			if T::Currency::mint_into(&rewardee, reward_size).is_err() {
				return;
			};
			Self::deposit_event(Event::OrderPlacerRewarded { order_placer: rewardee });
		}
	}
}

pub struct FixedReward<Balance: BalanceT, Amount: Get<Balance>>(PhantomData<(Balance, Amount)>);
impl<Balance: BalanceT, Amount: Get<Balance>> RewardSize<Balance> for FixedReward<Balance, Amount> {
	fn reward_size() -> Balance {
		Balance::default().saturating_add(Amount::get())
	}
}
