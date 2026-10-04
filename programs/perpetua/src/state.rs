use anchor_lang::prelude::*;

/// Satu pasar leverage di atas satu feed harga. Dibuat bebas-izin: siapa pun
/// bisa "menyalakan" pasar untuk sebuah feed yang belum ada.
#[account]
#[derive(InitSpace)]
pub struct Market {
    /// Pembuat pasar (penyetor likuiditas awal).
    pub authority: Pubkey,
    /// Feed harga (Pyth) yang jadi acuan. H-1: disimpan, belum dibaca.
    pub oracle_feed: Pubkey,
    /// Harga acuan saat ini, 6 desimal. H-1/H-2: di-set via price ingress
    /// (set_mark_price manual / update_price_from_pyth).
    pub mark_price: u64,
    /// Total notional posisi long & short (buat skew/likuiditas nanti).
    pub total_long: u64,
    pub total_short: u64,
    /// Bump PDA vault (kolam lamports lawan-dagang).
    pub vault_bump: u8,
    pub bump: u8,
}

/// Posisi satu trader di satu pasar (satu posisi per (owner, market) di v1).
#[account]
#[derive(InitSpace)]
pub struct Position {
    pub owner: Pubkey,
    pub market: Pubkey,
    pub is_long: bool,
    /// Notional = collateral * leverage (dalam lamports).
    pub size: u64,
    /// Jaminan terkunci (lamports).
    pub collateral: u64,
    /// Harga masuk, 6 desimal.
    pub entry_price: u64,
    pub bump: u8,
}

impl Position {
    /// PnL belum-terealisasi (lamports, bertanda) pada harga `mark`.
    /// Positif = untung buat trader. Long untung kalau harga naik; short sebaliknya.
    pub fn unrealized_pnl(&self, mark: u64) -> i128 {
        let entry = self.entry_price as i128;
        if entry == 0 {
            return 0;
        }
        let raw = (self.size as i128) * (mark as i128 - entry) / entry;
        if self.is_long {
            raw
        } else {
            -raw
        }
    }

    /// Ekuitas = jaminan + PnL belum-terealisasi (bisa negatif = bangkrut).
    pub fn equity(&self, mark: u64) -> i128 {
        self.collateral as i128 + self.unrealized_pnl(mark)
    }
}

/// Kolam lamports lawan-dagang (vAMM sederhana v1). Akun milik-program yang
/// menyimpan SOL; payout & bounty ditarik manual dari sini.
#[account]
#[derive(InitSpace)]
pub struct Vault {
    pub market: Pubkey,
    pub bump: u8,
}
