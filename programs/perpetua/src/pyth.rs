use anchor_lang::prelude::*;

use crate::{constants::*, error::PerpetuaError};

/// F2 jalur (c): baca akun PriceUpdateV2 milik Pyth Receiver TANPA SDK, SDK-nya narik
/// solana-program monolitik yang bentrok sama split-crate Agave 4.x kita (skew M-09).
/// Layout di-mirror dari pyth-solana-receiver-sdk 2.0.0 (price_update.rs) + pythnet-sdk
/// 3.0.0 (messages.rs), dan dicocokin ke akun asli on-chain (SOL/USD 7UVi…, 14 Okt 2026):
///
///   [0..8]    discriminator = sha256("account:PriceUpdateV2")[..8]
///   [8..40]   write_authority: Pubkey
///   [40]      verification_level (enum borsh): 0 = Partial{num_signatures: u8}, 1 = Full
///   [41..]    (Full) PriceFeedMessage: feed_id [u8;32], price i64, conf u64, exponent i32,
///             publish_time i64, prev_publish_time i64, ema_price i64, ema_conf u64
///             lalu posted_slot u64
pub const PRICE_UPDATE_V2_DISCRIMINATOR: [u8; 8] = [34, 241, 35, 99, 157, 126, 244, 205];

/// Offset PriceFeedMessage kalau verifikasi Full (tag 1 byte, tanpa num_signatures).
const MESSAGE_OFFSET_FULL: usize = 8 + 32 + 1;
/// feed_id + price + conf + exponent + publish_time = bagian message yang kita baca.
const MESSAGE_READ_LEN: usize = 32 + 8 + 8 + 4 + 8;

/// Harga Pyth mentah yang udah lolos cek identitas (owner/discriminator/Full/feed).
pub struct PythPrice {
    pub price: i64,
    pub conf: u64,
    pub exponent: i32,
    pub publish_time: i64,
}

/// Baca harga dari akun PriceUpdateV2 dan pastiin asli: owner = Pyth Receiver, discriminator
/// cocok, verifikasi Full (semua guardian Wormhole), feed_id = feed pasar. Umur harga NGGAK
/// dicek di sini, tiap instruksi punya aturan waktunya sendiri (update vs resolve).
pub fn read_verified_price(price_update: &AccountInfo, feed_id: &[u8; 32]) -> Result<PythPrice> {
    require_keys_eq!(
        *price_update.owner,
        PYTH_RECEIVER_PROGRAM_ID,
        PerpetuaError::OracleInvalidAccount
    );
    let data = price_update.try_borrow_data()?;
    require!(
        data.len() >= MESSAGE_OFFSET_FULL + MESSAGE_READ_LEN,
        PerpetuaError::OracleInvalidAccount
    );
    require!(
        data[..8] == PRICE_UPDATE_V2_DISCRIMINATOR,
        PerpetuaError::OracleInvalidAccount
    );
    require!(data[40] == 1, PerpetuaError::OracleNotFullyVerified);

    let msg = &data[MESSAGE_OFFSET_FULL..];
    require!(&msg[..32] == feed_id, PerpetuaError::OracleFeedMismatch);

    Ok(PythPrice {
        price: i64::from_le_bytes(le8(&msg[32..40])),
        conf: u64::from_le_bytes(le8(&msg[40..48])),
        exponent: i32::from_le_bytes(le4(&msg[48..52])),
        publish_time: i64::from_le_bytes(le8(&msg[52..60])),
    })
}

/// Harga Pyth -> 6 desimal (skala harga Perpetua). Tolak harga <= 0, conf kelebaran
/// (> MAX_CONF_BPS dari harga), dan overflow.
pub fn to_6dec_checked(p: &PythPrice) -> Result<u64> {
    require!(p.price > 0, PerpetuaError::InvalidPrice);
    let price = p.price as u128;
    // conf/price <= MAX_CONF_BPS/BPS  <=>  conf*BPS <= price*MAX_CONF_BPS (u128, nggak overflow)
    require!(
        (p.conf as u128) * (BPS_DENOMINATOR as u128) <= price * (MAX_CONF_BPS as u128),
        PerpetuaError::OracleConfidenceTooWide
    );

    let shift = p.exponent + 6;
    let scaled = if shift >= 0 {
        let mul = 10u128
            .checked_pow(shift as u32)
            .ok_or(PerpetuaError::MathOverflow)?;
        price.checked_mul(mul).ok_or(PerpetuaError::MathOverflow)?
    } else {
        let div = 10u128
            .checked_pow((-shift) as u32)
            .ok_or(PerpetuaError::MathOverflow)?;
        price / div
    };
    require!(scaled > 0, PerpetuaError::InvalidPrice);
    u64::try_from(scaled).map_err(|_| error!(PerpetuaError::MathOverflow))
}

fn le8(b: &[u8]) -> [u8; 8] {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[..8]);
    a
}

fn le4(b: &[u8]) -> [u8; 4] {
    let mut a = [0u8; 4];
    a.copy_from_slice(&b[..4]);
    a
}
