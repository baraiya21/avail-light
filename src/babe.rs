//! BABE consensus.
//!
//! BABE, or Blind Assignment for Blockchain Extension, is the consensus algorithm used by
//! Polkadot in order to determine who is authorized to generate a block.
//!
//! Every block (with the exception of the genesis block) must contain, in its header, some data
//! that makes it possible to verify that it has been generated by a legitimate author.
//!
//! References:
//!
//! - https://research.web3.foundation/en/latest/polkadot/BABE/Babe.html
//!
//! # Overview of BABE
//!
//! In the BABE algorithm, time is divided into non-overlapping **epochs**, themselves divided
//! into **slots**. How long an epoch and a slot are is determined by calling the
//! `BabeApi_configuration` runtime entry point.
//!
//! > **Note**: As example values, in the Polkadot genesis, a slot lasts for 6 seconds and an
//! >           epoch consists of 2400 slots (in other words, four hours).
//!
//! Every block that is produced must belong to a specific slot. This slot number can be found in
//! the block header, with the exception of the genesis block which is considered timeless and
//! doesn't have any slot number.
//!
//! At the moment, the current slot number is determined purely based on the slot duration (e.g.
//! 6 seconds for Polkadot) and the local clock based on the UNIX EPOCH. The current slot
//! number is `unix_timestamp / duration_per_slot`. This might change in the future.
//!
//! The first epoch ends at `slot_number(block #1) + slots_per_epoch - 1`. After that, all epochs
//! end at `end_of_previous_epoch + slots_per_epoch`.
//!
//! The header of first block produced after a transition to a new epoch must contain a log entry
//! indicating the public keys that are allowed to sign blocks, alongside with a weight for each of
//! them, and a "randomness value". This information does not concern the newly-started epoch, but
//! the one immediately after. In other words, the first block of epoch `N` contains the
//! information about epoch `N+1`.
//!
//! > **Note**: The way the list of authorities and their weights is determined is at the
//! >           discretion of the runtime code and is out of scope of this module, but it normally
//! >           corresponds to the list of validators and how much stake is available to them.
//!
//! In order to produce a block, one must generate, using a
//! [VRF (Verifiable Random Function)](https://en.wikipedia.org/wiki/Verifiable_random_function),
//! and based on the slot number, genesis hash, and aformentioned "randomness value",
//! a number whose value is lower than a certain threshold.
//!
//! The number that has been generated must be included in the header of the authored block,
//! alongside with the proof of the correct generation that can be verified using one of the
//! public keys allowed to generate blocks in that epoch. The weight associated to that public key
//! determines the allowed threshold.
//!
//! The "randomess value" of an epoch `N` is calculated by combining the generated numbers of all
//! the blocks of the epoch `N - 2`.
//!
//! ## Secondary slots
//!
//! While all slots can be claimed by generating a number below a certain threshold, each slot is
//! additionally assigned to a specific public key amongst the ones allowed. The owner of a
//! public key is always allowed to generate a block during the slot assigned to it.
//!
//! The mechanism of attributing each slot to a public key is called "secondary slots", while the
//! mechanism of generating a number below a certain threshold is called "primary slots". As their
//! name indicates, primary slots have a higher priority over secondary slots.
//!
//! Secondary slots are a way to guarantee that all slots can potentially lead to a block being
//! produced.
//!
//! ## Chain selection
//!
//! The "best" block of a chain in the BABE algorithm is the one with the highest slot number.
//! If there exists multiple blocks on the same slot, the best block is one with the highest number
//! of primary slots claimed. In other words, if two blocks have the same parent, but one is a
//! primary slot claim and the other is a secondary slot claim, we prefer the one with the primary
//! slot claim.
//!
//! Keep in mind that there can still be draws in terms of primary slot claims count, in which
//! case the winning block is the one upon which the next block author builds upon.
//!
//! ## Epochs 0 and 1
//!
//! The information about an epoch `N` is provided by the first block of the epoch `N-1`.
//!
//! Because of this, we need to special-case epochs 0 and 1. The information about these two
//! epochs in particular is contained in the chain-wide BABE configuration found in the runtime.
//!
//! # Usage
//!
//! Verifying a BABE block is done in two phases:
//!
//! - First, call [`start_verify_header`] to start the verification process. This returns a
//! [`SuccessOrPending`] enum.
//! - If [`SuccessOrPending::Pending`] has been returned, you need to provide a specific
//! [`EpochInformation`] struct.
//!

use crate::header;
use core::time::Duration;

mod definitions;
mod runtime;

pub mod chain_config;
pub mod header_info;

pub use chain_config::BabeGenesisConfiguration;

/// Configuration for [`start_verify_header`].
pub struct VerifyConfig<'a> {
    /// Header of the block to verify.
    pub header: header::HeaderRef<'a>,

    /// Time elapsed since [the Unix Epoch](https://en.wikipedia.org/wiki/Unix_time) (i.e.
    /// 00:00:00 UTC on 1 January 1970), ignoring leap seconds.
    // TODO: unused, should check against a block's slot
    pub now_from_unix_epoch: Duration,

    /// Header of the parent of the block to verify.
    ///
    /// [`start_verify_header`] assumes that this block has been successfully verified before.
    ///
    /// The hash of this header must be the one referenced in [`VerifyConfig::header`].
    pub parent_block_header: header::HeaderRef<'a>,

    /// BABE configuration retrieved from the genesis block.
    ///
    /// Can be obtained by calling [`BabeGenesisConfiguration::from_virtual_machine_prototype`]
    /// with the runtime of the genesis block.
    pub genesis_configuration: &'a BabeGenesisConfiguration,

    /// Slot number of block #1. **Must** be provided, unless the block being verified is block
    /// #1 itself.
    pub block1_slot_number: Option<u64>,
}

/// Information yielded back after successfully verifying a block.
#[derive(Debug)]
pub struct VerifySuccess {
    /// If `Some`, the verified block contains an epoch transition. This epoch transition must
    /// later be provided back as part of the [`VerifyConfig`] of the blocks that are part of
    /// that epoch.
    pub epoch_change: Option<EpochInformation>,

    /// Slot number the block belongs to.
    pub slot_number: u64,
}

/// Information about an epoch.
///
/// Obtained as part of the [`VerifySuccess`] returned after verifying a block.
#[derive(Debug)]
pub struct EpochInformation {
    /// List of authorities that are allowed to sign blocks during this epoch.
    ///
    /// The order of the authorities in the list is important, as blocks contain the index, in
    /// that list, of the authority that signed them.
    pub authorities: Vec<EpochInformationAuthority>,

    /// High-entropy data that can be used as a source of randomness during this epoch. Built
    /// by the runtime using the VRF output of all the blocks in the previous epoch.
    pub randomness: [u8; 32],
}

/// Information about a specific authority.
#[derive(Debug)]
pub struct EpochInformationAuthority {
    /// Ristretto public key that is authorized to sign blocks.
    pub public_key: [u8; 32],

    /// An arbitrary weight value applied to this authority.
    ///
    /// These values don't have any meaning in the absolute, only relative to each other. An
    /// authority with twice the weight value as another authority will be able to claim twice as
    /// many slots.
    pub weight: u64,
}

/// Failure to verify a block.
#[derive(Debug, derive_more::Display)]
pub enum VerifyError {
    /// Error while reading information from the header.
    BadHeader(header_info::Error),
    /// Slot number must be strictly increasing between a parent and its child.
    SlotNumberNotIncreasing,
    /// Block contains an epoch change digest log, but no epoch change is to be performed.
    UnexpectedEpochChangeLog,
    /// Block is the first block after a new epoch, but it is missing an epoch change digest log.
    MissingEpochChangeLog,
}

/// Verifies whether a block header provides a correct proof of the legitimacy of the authorship.
///
/// Returns either a [`PendingVerify`] if more information is needed, or a [`VerifySuccess`] if
/// the verification could be successfully performed.
///
/// # Panic
///
/// Panics if `config.parent_block_header` is invalid.
/// Panics if `config.block1_slot_number` is `None` and `config.header.number` is not 1.
///
pub fn start_verify_header<'a>(
    config: VerifyConfig<'a>,
) -> Result<SuccessOrPending<'a>, VerifyError> {
    let header =
        header_info::header_information(config.header.clone()).map_err(VerifyError::BadHeader)?;

    // Gather the BABE-related information.
    let (authority_index, slot_number, primary, vrf) = match header.pre_runtime {
        header_info::PreDigest::Primary(digest) => (
            digest.authority_index,
            digest.slot_number,
            true,
            Some((digest.vrf_output, digest.vrf_proof)),
        ),
        header_info::PreDigest::SecondaryPlain(digest) => {
            (digest.authority_index, digest.slot_number, false, None)
        }
        header_info::PreDigest::SecondaryVRF(digest) => (
            digest.authority_index,
            digest.slot_number,
            false,
            Some((digest.vrf_output, digest.vrf_proof)),
        ),
    };

    // Determine the epoch number of the block that we verify.
    let epoch_number = match (slot_number, config.block1_slot_number) {
        (curr, Some(block1)) => slot_number_to_epoch(curr, config.genesis_configuration, block1).unwrap(), // TODO: don't unwrap
        (_, None) if config.header.number == 1 => 0,
        (_, None) => panic!(),
    };

    // Determine the epoch number of the parent block.
    let parent_epoch_number = if config.parent_block_header.number != 0 {
        let parent_info = header_info::header_information(config.parent_block_header.clone()).unwrap();
        if config.parent_block_header.number != 1 {
            slot_number_to_epoch(parent_info.slot_number(), config.genesis_configuration, config.block1_slot_number.unwrap()).unwrap()
        } else {
            0
        }
    } else {
        0
    };

    let epoch_change = header
        .epoch_change
        .map(|(epoch_change, _)| EpochInformation {
            randomness: epoch_change.randomness,
            authorities: epoch_change
                .authorities
                .into_iter()
                .map(|(public_key, weight)| EpochInformationAuthority { public_key, weight })
                .collect(),
        });

    // Make sure that the expected epoch transitions corresponds to what the block reports.
    match (&epoch_change, epoch_number != parent_epoch_number) {
        (Some(_), true) => {},
        (None, false) => {},
        (Some(_), false) => { println!("num = {:?}", config.header.number); return Err(VerifyError::UnexpectedEpochChangeLog)},
        (None, true) => return Err(VerifyError::MissingEpochChangeLog),
    };

    // TODO: as a hack, we just return `Success` right now even though we don't check much; this
    //       is because the `Pending` variant is unusable
    Ok(SuccessOrPending::Success(VerifySuccess {
        epoch_change,
        slot_number,
    }))
}

/// Verification in progress. The block is **not** fully verified yet. You must call
/// [`PendingVerify::finish`] in order to finish the verification.
#[must_use]
pub enum SuccessOrPending<'a> {
    Pending(PendingVerify<'a>),
    Success(VerifySuccess),
}

/// Verification in progress. The block is **not** fully verified yet. You must call
/// [`PendingVerify::finish`] in order to finish the verification.
#[must_use]
pub struct PendingVerify<'a> {
    config: VerifyConfig<'a>,
}

impl<'a> PendingVerify<'a> {
    // TODO: should provide ways to find out which `EpochInformation` to pass back

    /// Finishes the verification. Must provide the information about the epoch the block belongs
    /// to.
    pub fn finish(self, epoch_info: &EpochInformation) -> Result<VerifySuccess, VerifyError> {
        let header = header_info::header_information(self.config.header.clone())
            .map_err(VerifyError::BadHeader)?;

        // Gather the BABE-related information.
        let (authority_index, slot_number, primary, vrf) = match header.pre_runtime {
            header_info::PreDigest::Primary(digest) => (
                digest.authority_index,
                digest.slot_number,
                true,
                Some((digest.vrf_output, digest.vrf_proof)),
            ),
            header_info::PreDigest::SecondaryPlain(digest) => {
                (digest.authority_index, digest.slot_number, false, None)
            }
            header_info::PreDigest::SecondaryVRF(digest) => (
                digest.authority_index,
                digest.slot_number,
                false,
                Some((digest.vrf_output, digest.vrf_proof)),
            ),
        };

        // Slot number of the parent block.
        let parent_slot_number = {
            let parent_info =
                header_info::header_information(self.config.parent_block_header).unwrap();
            match parent_info.pre_runtime {
                header_info::PreDigest::Primary(digest) => digest.slot_number,
                header_info::PreDigest::SecondaryPlain(digest) => digest.slot_number,
                header_info::PreDigest::SecondaryVRF(digest) => digest.slot_number,
            }
        };

        if slot_number <= parent_slot_number {
            return Err(VerifyError::SlotNumberNotIncreasing);
        }

        // TODO: gather current authorities, and verify everything

        // The signature in the seal applies to the header from where the signature isn't present.
        // Build the hash that is expected to be signed.
        let pre_seal_hash = {
            let mut unsealed_header = self.config.header;
            let _popped = unsealed_header.digest.pop();
            debug_assert!(matches!(_popped, Some(header::DigestItemRef::Seal(_, _))));
            unsealed_header.hash()
        };

        // TODO: check that epoch change is in header iff it's actually an epoch change

        // TODO: in case of epoch change, should also check the randomness value; while the runtime
        //       checks that the randomness value is correct, light clients in particular do not
        //       execute the runtime

        // TODO: handle config change
        let epoch_change = header
            .epoch_change
            .map(|(epoch_change, _)| EpochInformation {
                randomness: epoch_change.randomness,
                authorities: epoch_change
                    .authorities
                    .into_iter()
                    .map(|(public_key, weight)| EpochInformationAuthority { public_key, weight })
                    .collect(),
            });

        Ok(VerifySuccess {
            epoch_change,
            slot_number,
        })
    }
}

/// Turns a slot number into an epoch number.
///
/// Returns an error if `slot_number` is inferior to `block1_slot_number`.
fn slot_number_to_epoch(slot_number: u64, genesis_config: &BabeGenesisConfiguration, block1_slot_number: u64) -> Result<u64, ()> {
    let slots_diff = slot_number.checked_sub(block1_slot_number).ok_or(())?;
    Ok((slots_diff.checked_add(1).ok_or(())?) / genesis_config.slots_per_epoch())
}
