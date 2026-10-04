use anchor_lang::prelude::*;

#[error_code]
pub enum PerpetuaError {
    #[msg("Only the market authority may perform this action")]
    Unauthorized,
    #[msg("Leverage out of allowed range (1..=MAX_LEVERAGE)")]
    InvalidLeverage,
    #[msg("Collateral must be greater than zero")]
    ZeroCollateral,
    #[msg("Invalid market price (zero)")]
    InvalidPrice,
    #[msg("Liquidity pool has insufficient funds to cover the payout")]
    InsufficientLiquidity,
    #[msg("Arithmetic overflow")]
    MathOverflow,
    #[msg("Position is still healthy — not eligible for liquidation")]
    PositionHealthy,
}
