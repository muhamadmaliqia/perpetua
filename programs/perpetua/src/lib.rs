pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("GTaf7icvPHNzDMfd1gt8eN5p3fKERRwPaenfWKjGhaGP");

#[program]
pub mod perpetua {
    use super::*;

    /// Nyalakan pasar leverage baru untuk sebuah feed — bebas-izin.
    pub fn create_market(
        ctx: Context<CreateMarket>,
        oracle_feed: Pubkey,
        initial_price: u64,
        initial_liquidity: u64,
    ) -> Result<()> {
        crate::instructions::create_market::handle_create_market(
            ctx,
            oracle_feed,
            initial_price,
            initial_liquidity,
        )
    }

    /// PLACEHOLDER H-1/H-2: set harga acuan manual (authority). Dipakai buat test
    /// deterministik & fallback demo. H-2+: harga asli via update_price_from_pyth.
    pub fn set_mark_price(ctx: Context<SetMarkPrice>, price: u64) -> Result<()> {
        crate::instructions::set_mark_price::handle_set_mark_price(ctx, price)
    }

    /// Buka posisi long/short dengan pengungkit.
    pub fn open_position(
        ctx: Context<OpenPosition>,
        collateral: u64,
        leverage: u64,
        is_long: bool,
    ) -> Result<()> {
        crate::instructions::open_position::handle_open_position(ctx, collateral, leverage, is_long)
    }

    /// Tutup posisi: hitung PnL vs harga acuan, bayar payout dari kolam.
    pub fn close_position(ctx: Context<ClosePosition>) -> Result<()> {
        crate::instructions::close_position::handle_close_position(ctx)
    }

    /// Likuidasi permissionless: tutup paksa posisi yang margin-nya jebol,
    /// liquidator dapat bounty dari kolam.
    pub fn liquidate_position(ctx: Context<LiquidatePosition>) -> Result<()> {
        crate::instructions::liquidate_position::handle_liquidate_position(ctx)
    }
}
