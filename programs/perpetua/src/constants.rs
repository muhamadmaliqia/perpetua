use anchor_lang::prelude::*;

#[constant]
pub const MARKET_SEED: &[u8] = b"market";

#[constant]
pub const POSITION_SEED: &[u8] = b"position";

#[constant]
pub const VAULT_SEED: &[u8] = b"vault";

/// Harga & rasio pakai 6 desimal tetap (1_000_000 = 1.0).
#[constant]
pub const PRICE_ONE: u64 = 1_000_000;

/// Batas pengungkit demo v1 — dijaga sempit biar fokus (M-04).
#[constant]
pub const MAX_LEVERAGE: u64 = 20;

/// Penyebut basis-point (10_000 = 100%).
#[constant]
pub const BPS_DENOMINATOR: u64 = 10_000;

/// Maintenance margin: kalau ekuitas ≤ mmr% dari notional, posisi bisa dilikuidasi. 500 = 5%.
#[constant]
pub const MAINTENANCE_MARGIN_BPS: u64 = 500;

/// Bounty buat liquidator, dari notional. 100 = 1%.
#[constant]
pub const LIQUIDATION_BOUNTY_BPS: u64 = 100;
