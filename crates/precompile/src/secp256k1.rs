use crate::{
    utilities::right_pad, PrecompileError, PrecompileOutput, PrecompileResult,
    PrecompileWithAddress,
};
use primitives::{alloy_primitives::B512, Bytes, B256};

pub const ECRECOVER: PrecompileWithAddress =
    PrecompileWithAddress(crate::u64_to_address(1), ec_recover_run);

pub use self::secp256k1::ecrecover;

#[allow(clippy::module_inception)]
mod secp256k1 {
    use primitives::{alloy_primitives::B512, keccak256, B256};

    cfg_if::cfg_if! {
        if #[cfg(feature = "secp256k1")] {
            use secp256k1::{
                ecdsa::{RecoverableSignature, RecoveryId},
                Message, SECP256K1,
            };

            // Silence the unused crate dependency warning.
            use k256 as _;

            pub fn ecrecover(sig: &B512, recid: u8, msg: &B256) -> Result<B256, secp256k1::Error> {
                let recid = RecoveryId::from_i32(recid as i32).expect("recovery ID is valid");
                let sig = RecoverableSignature::from_compact(sig.as_slice(), recid)?;

                let msg = Message::from_digest(msg.0);
                let public = SECP256K1.recover_ecdsa(&msg, &sig)?;

                let mut hash = keccak256(&public.serialize_uncompressed()[1..]);
                hash[..12].fill(0);
                Ok(hash)
            }
        } else if #[cfg(feature = "libsecp256k1")] {
            pub fn ecrecover(sig: &B512, recid: u8, msg: &B256) -> Result<B256, libsecp256k1::Error> {
                let recid = libsecp256k1::RecoveryId::parse(recid)?;
                let sig = RecoverableSignature::from_compact(sig.as_slice(), recid)?;

                let msg = libsecp256k1::Message::parse(msg.as_ref());
                let public = libsecp256k1::recover(&msg, &sig, &recid)?;

                let mut hash = keccak256(&public.serialize_uncompressed()[1..]);
                hash[..12].fill(0);
                Ok(hash)
            }
        } else {
            use k256::ecdsa::{Error, RecoveryId, Signature, VerifyingKey};

            pub fn ecrecover(sig: &B512, mut recid: u8, msg: &B256) -> Result<B256, Error> {
                // parse signature
                let mut sig = Signature::from_slice(sig.as_slice())?;

                // normalize signature and flip recovery id if needed.
                if let Some(sig_normalized) = sig.normalize_s() {
                    sig = sig_normalized;
                    recid ^= 1;
                }
                let recid = RecoveryId::from_byte(recid).expect("recovery ID is valid");

                // recover key
                let recovered_key = VerifyingKey::recover_from_prehash(&msg[..], &sig, recid)?;
                // hash it
                let mut hash = keccak256(
                    &recovered_key
                        .to_encoded_point(/* compress = */ false)
                        .as_bytes()[1..],
                );

                // truncate to 20 bytes
                hash[..12].fill(0);
                Ok(hash)
            }
        }
    }
}

pub fn ec_recover_run(input: &Bytes, gas_limit: u64) -> PrecompileResult {
    const ECRECOVER_BASE: u64 = 3_000;

    if ECRECOVER_BASE > gas_limit {
        return Err(PrecompileError::OutOfGas.into());
    }

    let input = right_pad::<128>(input);

    // `v` must be a 32-byte big-endian integer equal to 27 or 28.
    if !(input[32..63].iter().all(|&b| b == 0) && matches!(input[63], 27 | 28)) {
        return Ok(PrecompileOutput::new(ECRECOVER_BASE, Bytes::new()));
    }

    let msg = <&B256>::try_from(&input[0..32]).unwrap();
    let recid = input[63] - 27;
    let sig = <&B512>::try_from(&input[64..128]).unwrap();

    let out = secp256k1::ecrecover(sig, recid, msg)
        .map(|o| o.to_vec().into())
        .unwrap_or_default();
    Ok(PrecompileOutput::new(ECRECOVER_BASE, out))
}
