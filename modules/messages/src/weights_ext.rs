// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Weight-related utilities.

use crate::weights::WeightInfo;

use bp_messages::{MessageNonce, UnrewardedRelayersState};
use bp_runtime::{PreComputedSize, Size};
use frame_support::weights::Weight;

/// Size of the message being delivered in benchmarks.
pub const EXPECTED_DEFAULT_MESSAGE_LENGTH: u32 = 128;

/// We assume that size of signed extensions on all our chains and size of all 'small' arguments of
/// calls we're checking here would fit 1KB.
const SIGNED_EXTENSIONS_SIZE: u32 = 1024;

/// Number of extra bytes (excluding size of storage value itself) of storage proof, built at
/// Rialto chain. This mostly depends on number of entries (and their density) in the storage trie.
/// Some reserve is reserved to account future chain growth.
pub const EXTRA_STORAGE_PROOF_SIZE: u32 = 1024;

/// Ensure that weights from `WeightInfoExt` implementation are looking correct.
pub fn ensure_weights_are_correct<W: WeightInfoExt>() {
	// verify `receive_messages_proof` weight components
	assert_ne!(W::receive_messages_proof_overhead(), Weight::zero());
	assert_ne!(W::receive_messages_proof_messages_overhead(1), Weight::zero());
	assert_ne!(W::receive_messages_proof_outbound_lane_state_overhead(), Weight::zero());
	assert_ne!(W::storage_proof_size_overhead(1), Weight::zero());

	// verify `receive_messages_delivery_proof` weight components
	assert_ne!(W::receive_messages_delivery_proof_overhead(), Weight::zero());
	assert_ne!(W::storage_proof_size_overhead(1), Weight::zero());
}

/// Ensure that we're able to receive maximal (by-size and by-weight) message from other chain.
pub fn ensure_able_to_receive_message<W: WeightInfoExt>(
	max_extrinsic_size: u32,
	max_extrinsic_weight: Weight,
	max_incoming_message_proof_size: u32,
	max_incoming_message_dispatch_weight: Weight,
) {
	// verify that we're able to receive proof of maximal-size message
	let max_delivery_transaction_size =
		max_incoming_message_proof_size.saturating_add(SIGNED_EXTENSIONS_SIZE);
	assert!(
		max_delivery_transaction_size <= max_extrinsic_size,
		"Size of maximal message delivery transaction {max_incoming_message_proof_size} + {SIGNED_EXTENSIONS_SIZE} is larger than maximal possible transaction size {max_extrinsic_size}",
	);

	// verify that we're able to receive proof of maximal-size message with maximal dispatch weight
	let max_delivery_transaction_dispatch_weight = W::receive_messages_proof_weight(
		&PreComputedSize(
			(max_incoming_message_proof_size + W::expected_extra_storage_proof_size()) as usize,
		),
		1,
		max_incoming_message_dispatch_weight,
	);
	assert!(
		max_delivery_transaction_dispatch_weight.all_lte(max_extrinsic_weight),
		"Weight of maximal message delivery transaction + {max_delivery_transaction_dispatch_weight} is larger than maximal possible transaction weight {max_extrinsic_weight}",
	);
}

/// Ensure that we're able to receive maximal confirmation from other chain.
pub fn ensure_able_to_receive_confirmation<W: WeightInfoExt>(
	max_extrinsic_size: u32,
	max_extrinsic_weight: Weight,
	max_inbound_lane_data_proof_size_from_peer_chain: u32,
	max_unrewarded_relayer_entries_at_peer_inbound_lane: MessageNonce,
	max_unconfirmed_messages_at_inbound_lane: MessageNonce,
) {
	// verify that we're able to receive confirmation of maximal-size
	let max_confirmation_transaction_size =
		max_inbound_lane_data_proof_size_from_peer_chain.saturating_add(SIGNED_EXTENSIONS_SIZE);
	assert!(
		max_confirmation_transaction_size <= max_extrinsic_size,
		"Size of maximal message delivery confirmation transaction {max_inbound_lane_data_proof_size_from_peer_chain} + {SIGNED_EXTENSIONS_SIZE} is larger than maximal possible transaction size {max_extrinsic_size}",
	);

	// verify that we're able to reward maximal number of relayers that have delivered maximal
	// number of messages
	let max_confirmation_transaction_dispatch_weight = W::receive_messages_delivery_proof_weight(
		&PreComputedSize(max_inbound_lane_data_proof_size_from_peer_chain as usize),
		&UnrewardedRelayersState {
			unrewarded_relayer_entries: max_unrewarded_relayer_entries_at_peer_inbound_lane,
			total_messages: max_unconfirmed_messages_at_inbound_lane,
			..Default::default()
		},
	);
	assert!(
		max_confirmation_transaction_dispatch_weight.all_lte(max_extrinsic_weight),
		"Weight of maximal confirmation transaction {max_confirmation_transaction_dispatch_weight} is larger than maximal possible transaction weight {max_extrinsic_weight}",
	);
}

/// Extended weight info.
pub trait WeightInfoExt: WeightInfo {
	/// Size of proof that is already included in the single message delivery weight.
	///
	/// The message submitter (at source chain) has already covered this cost. But there are two
	/// factors that may increase proof size: (1) the message size may be larger than predefined
	/// and (2) relayer may add extra trie nodes to the proof. So if proof size is larger than
	/// this value, we're going to charge relayer for that.
	fn expected_extra_storage_proof_size() -> u32;

	// Functions that are directly mapped to extrinsics weights.

	/// Weight of message delivery extrinsic.
	fn receive_messages_proof_weight(
		proof: &impl Size,
		messages_count: u32,
		dispatch_weight: Weight,
	) -> Weight {
		// basic components of extrinsic weight
		let transaction_overhead = Self::receive_messages_proof_overhead();
		let outbound_state_delivery_weight =
			Self::receive_messages_proof_outbound_lane_state_overhead();
		let messages_delivery_weight =
			Self::receive_messages_proof_messages_overhead(MessageNonce::from(messages_count));
		let messages_dispatch_weight = dispatch_weight;

		// proof size overhead weight
		let expected_proof_size = EXPECTED_DEFAULT_MESSAGE_LENGTH
			.saturating_mul(messages_count.saturating_sub(1))
			.saturating_add(Self::expected_extra_storage_proof_size());
		let actual_proof_size = proof.size();
		let proof_size_overhead = Self::storage_proof_size_overhead(
			actual_proof_size.saturating_sub(expected_proof_size),
		);

		transaction_overhead
			.saturating_add(outbound_state_delivery_weight)
			.saturating_add(messages_delivery_weight)
			.saturating_add(messages_dispatch_weight)
			.saturating_add(proof_size_overhead)
	}

	/// Weight of confirmation delivery extrinsic.
	fn receive_messages_delivery_proof_weight(
		proof: &impl Size,
		relayers_state: &UnrewardedRelayersState,
	) -> Weight {
		// basic components of extrinsic weight
		let transaction_overhead = Self::receive_messages_delivery_proof_overhead();
		let messages_overhead =
			Self::receive_messages_delivery_proof_messages_overhead(relayers_state.total_messages);
		let relayers_overhead = Self::receive_messages_delivery_proof_relayers_overhead(
			relayers_state.unrewarded_relayer_entries,
		);

		// proof size overhead weight
		let expected_proof_size = Self::expected_extra_storage_proof_size();
		let actual_proof_size = proof.size();
		let proof_size_overhead = Self::storage_proof_size_overhead(
			actual_proof_size.saturating_sub(expected_proof_size),
		);

		transaction_overhead
			.saturating_add(messages_overhead)
			.saturating_add(relayers_overhead)
			.saturating_add(proof_size_overhead)
	}

	// Functions that are used by extrinsics weights formulas.

	/// Returns weight overhead of message delivery transaction (`receive_messages_proof`).
	fn receive_messages_proof_overhead() -> Weight {
		let weight_of_two_messages_and_two_tx_overheads =
			Self::receive_single_message_proof().saturating_mul(2);
		let weight_of_two_messages_and_single_tx_overhead = Self::receive_two_messages_proof();
		weight_of_two_messages_and_two_tx_overheads
			.saturating_sub(weight_of_two_messages_and_single_tx_overhead)
	}

	/// Returns weight that needs to be accounted when receiving given a number of messages with
	/// message delivery transaction (`receive_messages_proof`).
	fn receive_messages_proof_messages_overhead(messages: MessageNonce) -> Weight {
		let weight_of_two_messages_and_single_tx_overhead = Self::receive_two_messages_proof();
		let weight_of_single_message_and_single_tx_overhead = Self::receive_single_message_proof();
		weight_of_two_messages_and_single_tx_overhead
			.saturating_sub(weight_of_single_message_and_single_tx_overhead)
			.saturating_mul(messages as _)
	}

	/// Returns weight that needs to be accounted when message delivery transaction
	/// (`receive_messages_proof`) is carrying outbound lane state proof.
	fn receive_messages_proof_outbound_lane_state_overhead() -> Weight {
		let weight_of_single_message_and_lane_state =
			Self::receive_single_message_proof_with_outbound_lane_state();
		let weight_of_single_message = Self::receive_single_message_proof();
		weight_of_single_message_and_lane_state.saturating_sub(weight_of_single_message)
	}

	/// Returns weight overhead of delivery confirmation transaction
	/// (`receive_messages_delivery_proof`).
	fn receive_messages_delivery_proof_overhead() -> Weight {
		let weight_of_two_messages_and_two_tx_overheads =
			Self::receive_delivery_proof_for_single_message().saturating_mul(2);
		let weight_of_two_messages_and_single_tx_overhead =
			Self::receive_delivery_proof_for_two_messages_by_single_relayer();
		weight_of_two_messages_and_two_tx_overheads
			.saturating_sub(weight_of_two_messages_and_single_tx_overhead)
	}

	/// Returns weight that needs to be accounted when receiving confirmations for given a number of
	/// messages with delivery confirmation transaction (`receive_messages_delivery_proof`).
	fn receive_messages_delivery_proof_messages_overhead(messages: MessageNonce) -> Weight {
		let weight_of_two_messages =
			Self::receive_delivery_proof_for_two_messages_by_single_relayer();
		let weight_of_single_message = Self::receive_delivery_proof_for_single_message();
		weight_of_two_messages
			.saturating_sub(weight_of_single_message)
			.saturating_mul(messages as _)
	}

	/// Returns weight that needs to be accounted when receiving confirmations for given a number of
	/// relayers entries with delivery confirmation transaction (`receive_messages_delivery_proof`).
	fn receive_messages_delivery_proof_relayers_overhead(relayers: MessageNonce) -> Weight {
		let weight_of_two_messages_by_two_relayers =
			Self::receive_delivery_proof_for_two_messages_by_two_relayers();
		let weight_of_two_messages_by_single_relayer =
			Self::receive_delivery_proof_for_two_messages_by_single_relayer();
		weight_of_two_messages_by_two_relayers
			.saturating_sub(weight_of_two_messages_by_single_relayer)
			.saturating_mul(relayers as _)
	}

	/// Returns weight that needs to be accounted when storage proof of given size is received
	/// (either in `receive_messages_proof` or `receive_messages_delivery_proof`).
	///
	/// **IMPORTANT**: this overhead is already included in the 'base' transaction cost - e.g. proof
	/// size depends on messages count or number of entries in the unrewarded relayers set. So this
	/// shouldn't be added to cost of transaction, but instead should act as a minimal cost that the
	/// relayer must pay when it relays proof of given size (even if cost based on other parameters
	/// is less than that cost).
	fn storage_proof_size_overhead(proof_size: u32) -> Weight {
		let proof_size_in_bytes = proof_size;
		let byte_weight = (Self::receive_single_message_proof_16_kb() -
			Self::receive_single_message_proof_1_kb()) /
			(15 * 1024);
		proof_size_in_bytes * byte_weight
	}
}

impl WeightInfoExt for () {
	fn expected_extra_storage_proof_size() -> u32 {
		EXTRA_STORAGE_PROOF_SIZE
	}
}

impl<T: frame_system::Config> WeightInfoExt for crate::weights::BridgeWeight<T> {
	fn expected_extra_storage_proof_size() -> u32 {
		EXTRA_STORAGE_PROOF_SIZE
	}
}
