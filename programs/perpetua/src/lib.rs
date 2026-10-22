pub mod constants;
pub mod error;
pub mod instructions;
pub mod pyth;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("GTaf7icvPHNzDMfd1gt8eN5p3fKERRwPaenfWKjGhaGP");

#[program]
pub mod perpetua {
    use super::*;

    /// Nyalakan pasar leverage baru untuk sebuah feed, bebas-izin.
    pub fn create_market(
        ctx: Context<CreateMarket>,
        oracle_feed: Pubkey,
        initial_price: u64,
        initial_liquidity: u64,
        skew_scale: u64,
        market_kind: u8,
        expiry_ts: i64,
    ) -> Result<()> {
        crate::instructions::create_market::handle_create_market(
            ctx,
            oracle_feed,
            initial_price,
            initial_liquidity,
            skew_scale,
            market_kind,
            expiry_ts,
        )
    }

    /// Override demo: set harga acuan manual (authority), drama pump/crash & test
    /// deterministik. Cuma perp yg BELUM pernah dapet harga Pyth; prediction market selalu
    /// ditolak. Harga Pyth pertama mensiunkan override ini permanen. Nggak dipakai buat resolve.
    pub fn set_mark_price(ctx: Context<SetMarkPrice>, price: u64) -> Result<()> {
        crate::instructions::set_mark_price::handle_set_mark_price(ctx, price)
    }

    /// F2: dorong harga Pyth terverifikasi ke mark, PERMISSIONLESS (keeper siapa pun).
    /// Kontrak sendiri yang verifikasi akun Pyth: owner, Full, feed, umur, conf.
    pub fn update_mark_from_pyth(ctx: Context<UpdateMarkFromPyth>) -> Result<()> {
        crate::instructions::update_mark_from_pyth::handle_update_mark_from_pyth(ctx)
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

    /// F3: resolve prediction market, PERMISSIONLESS, pasca jatuh tempo. Kontrak baca harga
    /// Pyth terverifikasi (terbit di [expiry, expiry+jendela]) dan kunci jadi putusan.
    /// Nggak ada komite/manusia, dan pembuat pasar nggak bisa ngarang harganya.
    pub fn resolve_market(ctx: Context<ResolveMarket>) -> Result<()> {
        crate::instructions::resolve_market::handle_resolve_market(ctx)
    }
}
