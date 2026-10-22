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

/// Batas pengungkit demo v1, dijaga sempit biar fokus (M-04).
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

/// Skala internal funding (resolusi index). 1e9 = 1.0.
pub const FUNDING_SCALE: i128 = 1_000_000_000;

/// Funding velocity maksimum (FUNDING_SCALE per detik^2) saat skew penuh (|prop_skew|=1).
/// Makin timpang long/short -> funding_rate makin ngebut (SIP-279). Ditune buat demo.
#[constant]
pub const MAX_FUNDING_VELOCITY: i64 = 15;

/// F3 market_kind: pasar perp klasik (perpetual, expiry=0, settle live mark price).
#[constant]
pub const MARKET_KIND_PERP: u8 = 0;

/// F3 market_kind: prediction market = perp ber-expiry yang self-resolve dari harga Pyth
/// terverifikasi pas jatuh tempo (lihat resolve_market). Satu primitif, bukan mesin baru.
#[constant]
pub const MARKET_KIND_PREDICTION: u8 = 1;

/// F2: program Pyth Receiver = pemilik akun PriceUpdateV2 (harga terverifikasi Wormhole).
/// Terukur on-chain 14 Okt 2026: akun SOL/USD sponsored 7UVi… di devnet & mainnet owner-nya ini.
pub const PYTH_RECEIVER_PROGRAM_ID: Pubkey = pubkey!("rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ");

/// F2: umur maksimum harga Pyth buat update mark perp (detik). Lebih tua = ditolak (StaleOracle).
#[constant]
pub const MAX_PRICE_AGE_SECONDS: i64 = 120;

/// F2: lebar ketidakpastian (conf) maksimum dari harga, dalam bps. 100 = 1%.
#[constant]
pub const MAX_CONF_BPS: u64 = 100;

/// F2+F3: harga Pyth buat resolve harus TERBIT di rentang [expiry, expiry + jendela].
/// 2 jam = cukup buat akun sponsored yang update-nya jarang (terukur ~1 jam, 14 Okt 2026).
#[constant]
pub const RESOLVE_WINDOW_SECONDS: i64 = 7200;
