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
    /// Total notional posisi long & short (open interest per sisi).
    pub total_long: u64,
    pub total_short: u64,
    /// Kedalaman pasar (virtual AMM): makin besar -> makin kecil price-impact per
    /// unit skew. Fill = oracle * (1 + skew/skew_scale), skew = total_long - total_short.
    /// Diset pembuat pasar (permissionless depth).
    pub skew_scale: u64,
    /// Funding velocity (SIP-279): funding_rate geser sebanding skew; index akumulasi
    /// tiap interaksi. cumulative_funding = index funding (6-dec fraksi), funding_rate =
    /// 6-dec/detik, last_funding_ts = detik terakhir di-akru (jam on-chain).
    pub cumulative_funding: i128,
    pub funding_rate: i128,
    pub last_funding_ts: i64,
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
    /// Snapshot cumulative_funding pas posisi dibuka (buat settle funding di close).
    pub entry_funding: i128,
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

    /// Funding yang jatuh tempo pada index `cumulative_funding` sekarang.
    /// + = posisi BAYAR (kurangi payout), - = posisi TERIMA. Long bayar pas pasar long-heavy.
    pub fn funding_owed(&self, cumulative_funding: i128) -> i128 {
        let signed_size = if self.is_long {
            self.size as i128
        } else {
            -(self.size as i128)
        };
        signed_size.saturating_mul(cumulative_funding - self.entry_funding) / crate::constants::FUNDING_SCALE
    }
}

impl Market {
    /// Akru funding sampai `now` (SIP-279). Dipanggil PERTAMA tiap interaksi (open/close/
    /// liquidate) sebelum OI berubah. funding_rate geser dgn velocity sebanding skew; index
    /// naik dari rata-rata rate x selang waktu. Semua saturating (nggak boleh panik on-chain).
    pub fn accrue_funding(&mut self, now: i64) {
        let dt = now.saturating_sub(self.last_funding_ts);
        if dt <= 0 {
            self.last_funding_ts = now;
            return;
        }
        let scale = crate::constants::FUNDING_SCALE;
        let skew = (self.total_long as i128) - (self.total_short as i128);
        // prop_skew (skala FUNDING_SCALE) di-clamp ke [-1, 1].
        let mut prop = skew.saturating_mul(scale) / (self.skew_scale as i128);
        if prop > scale {
            prop = scale;
        } else if prop < -scale {
            prop = -scale;
        }
        // velocity = perubahan funding_rate per detik (sebanding skew).
        let velocity = (crate::constants::MAX_FUNDING_VELOCITY as i128).saturating_mul(prop) / scale;
        let new_rate = self
            .funding_rate
            .saturating_add(velocity.saturating_mul(dt as i128));
        let avg_rate = (self.funding_rate + new_rate) / 2;
        self.cumulative_funding = self
            .cumulative_funding
            .saturating_add(avg_rate.saturating_mul(dt as i128));
        self.funding_rate = new_rate;
        self.last_funding_ts = now;
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
