use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{clock::Clock, instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_account::Account,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

// Harga 6 desimal: 1_000_000 = 1.0
const PRICE_100: u64 = 100_000_000; // $100.000000
const PRICE_110: u64 = 110_000_000; // $110.000000 (+10%)
const PRICE_82: u64 = 82_000_000; //  $82.000000  (-18%)
const SOL: u64 = 1_000_000_000;
// Kedalaman pasar buat skew-premium fill (B1). Raksasa = premium≈0 (angka regresi lama kejaga);
// demo = price-impact keliatan.
const SKEW_BIG: u64 = 100_000_000 * SOL;
const SKEW_DEMO: u64 = 25 * SOL;

fn setup() -> (LiteSVM, Pubkey) {
    let program_id = perpetua::id();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/perpetua.so"
    ));
    svm.add_program(program_id, bytes).unwrap();
    (svm, program_id)
}

fn send(svm: &mut LiteSVM, ix: Instruction, payer: &Keypair) {
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]).unwrap();
    let res = svm.send_transaction(tx);
    assert!(res.is_ok(), "tx gagal: {:?}", res.err());
}

// Perp: slot (feed, expiry 0).
fn market_pdas(program_id: &Pubkey, feed: &Pubkey, user: &Pubkey) -> (Pubkey, Pubkey, Pubkey) {
    market_pdas_exp(program_id, feed, user, 0)
}

// Slot pasar = ["market", feed, expiry_ts i64 LE]: perp = expiry 0, prediction = jatuh temponya.
fn market_pdas_exp(program_id: &Pubkey, feed: &Pubkey, user: &Pubkey, expiry_ts: i64) -> (Pubkey, Pubkey, Pubkey) {
    let (market, _) = Pubkey::find_program_address(
        &[perpetua::constants::MARKET_SEED, feed.as_ref(), &expiry_ts.to_le_bytes()],
        program_id,
    );
    let (vault, _) = Pubkey::find_program_address(
        &[perpetua::constants::VAULT_SEED, market.as_ref()],
        program_id,
    );
    let (position, _) = Pubkey::find_program_address(
        &[
            perpetua::constants::POSITION_SEED,
            market.as_ref(),
            user.as_ref(),
        ],
        program_id,
    );
    (market, vault, position)
}

fn load_market(svm: &LiteSVM, key: &Pubkey) -> perpetua::state::Market {
    let acc = svm.get_account(key).expect("market account tidak ada");
    let mut data: &[u8] = &acc.data;
    perpetua::state::Market::try_deserialize(&mut data).unwrap()
}

fn load_position(svm: &LiteSVM, key: &Pubkey) -> perpetua::state::Position {
    let acc = svm.get_account(key).expect("position account tidak ada");
    let mut data: &[u8] = &acc.data;
    perpetua::state::Position::try_deserialize(&mut data).unwrap()
}

/// Kirim tx yang MESTI gagal dengan error program tertentu (kode Anchor = 6000 + urutan enum).
/// Blockhash di-expire sebelum & sesudah: penolakan nggak ketuker sama AlreadyProcessed, dan tx
/// sah berikutnya (sering byte-identik sama percobaan gagal ini) dapet blockhash baru.
fn expect_err(svm: &mut LiteSVM, ix: Instruction, payer: &Keypair, code: u32, what: &str) {
    svm.expire_blockhash();
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]).unwrap();
    let res = svm.send_transaction(tx);
    svm.expire_blockhash();
    match res {
        Ok(_) => panic!("{}: tx mestinya DITOLAK dgn Custom({})", what, code),
        Err(e) => {
            let got = format!("{:?}", e.err);
            assert!(
                got.contains(&format!("Custom({})", code)),
                "{}: mau Custom({}), dapet {}",
                what,
                code,
                got
            );
        }
    }
}

fn err_code(e: perpetua::error::PerpetuaError) -> u32 {
    6000 + e as u32
}

fn set_clock(svm: &mut LiteSVM, unix_ts: i64) {
    let mut clock: Clock = svm.get_sysvar();
    clock.unix_timestamp = unix_ts;
    svm.set_sysvar(&clock);
}

/// Akun PriceUpdateV2 ASLI dari devnet (SOL/USD sponsored 7UVi…, owner Pyth Receiver), diambil
/// 14 Okt 2026 via getAccountInfo. Parser kontrak diuji ke byte beneran, bukan fixture karangan
/// sendiri: price=10745547506 expo=-8 ($107.45547506), conf=3012360, publish_time=1791921644.
const REAL_PYTH_SOL_USD: [u8; 134] = [
     34,241, 35, 99,157,126,244,205, 96, 49, 71,  4, 52, 13,237,223,
     55, 31,212, 36,114, 20,143, 36,142,157, 26,109, 26, 94,178,172,
     58,205,139,127,213,214,178, 67,  1,239, 13,139,111,218, 44,235,
    164, 29,161, 93, 64,149,209,218, 57, 42, 13, 47,142,208,198,199,
    188, 15, 76,250,200,194,128,181,109,242, 10,124,128,  2,  0,  0,
      0,  8,247, 45,  0,  0,  0,  0,  0,248,255,255,255,236,141,206,
    106,  0,  0,  0,  0,235,141,206,106,  0,  0,  0,  0, 84, 62,244,
    128,  2,  0,  0,  0,236,104, 48,  0,  0,  0,  0,  0,189, 65,113,
     30,  0,  0,  0,  0,  0,
];
const REAL_PYTH_PUBLISH_TIME: i64 = 1_791_921_644;

/// Data akun PriceUpdateV2 (layout Pyth Receiver, 134 byte). full=false -> Partial{5 sig}.
fn pyth_data(feed: &Pubkey, price: i64, conf: u64, expo: i32, publish_time: i64, full: bool) -> Vec<u8> {
    let mut d = Vec::with_capacity(134);
    d.extend_from_slice(&perpetua::pyth::PRICE_UPDATE_V2_DISCRIMINATOR);
    d.extend_from_slice(&[7u8; 32]); // write_authority
    if full {
        d.push(1);
    } else {
        d.push(0);
        d.push(5);
    }
    d.extend_from_slice(feed.as_ref());
    d.extend_from_slice(&price.to_le_bytes());
    d.extend_from_slice(&conf.to_le_bytes());
    d.extend_from_slice(&expo.to_le_bytes());
    d.extend_from_slice(&publish_time.to_le_bytes());
    d.extend_from_slice(&(publish_time - 1).to_le_bytes()); // prev_publish_time
    d.extend_from_slice(&price.to_le_bytes()); // ema_price
    d.extend_from_slice(&conf.to_le_bytes()); // ema_conf
    d.extend_from_slice(&0u64.to_le_bytes()); // posted_slot
    d.resize(134, 0);
    d
}

/// Pasang akun harga di `key` (owner asli: Pyth Receiver; owner lain = akun palsu).
fn put_pyth(svm: &mut LiteSVM, key: Pubkey, data: Vec<u8>, owner: Pubkey) {
    svm.set_account(
        key,
        Account { lamports: SOL, data, owner, executable: false, rent_epoch: 0 },
    )
    .unwrap();
}

/// Harga Pyth dgn expo -8 (format asli feed USD): dolar -> integer mentah.
fn pyth_usd(dollars: i64) -> i64 {
    dollars * 100_000_000
}

// Perp default (kind=0, expiry=0), 6 test lama manggil ini, NOL berubah.
fn ix_create_market(program_id: Pubkey, user: &Pubkey, feed: Pubkey, market: Pubkey, vault: Pubkey, price: u64, liq: u64, skew_scale: u64) -> Instruction {
    ix_create_market_kind(program_id, user, feed, market, vault, price, liq, skew_scale, perpetua::constants::MARKET_KIND_PERP, 0)
}

// Umum: perp (kind=0, expiry=0) ATAU prediction (kind=1, expiry>now). F3.
fn ix_create_market_kind(program_id: Pubkey, user: &Pubkey, feed: Pubkey, market: Pubkey, vault: Pubkey, price: u64, liq: u64, skew_scale: u64, market_kind: u8, expiry_ts: i64) -> Instruction {
    Instruction::new_with_bytes(
        program_id,
        &perpetua::instruction::CreateMarket {
            oracle_feed: feed,
            initial_price: price,
            initial_liquidity: liq,
            skew_scale,
            market_kind,
            expiry_ts,
        }
        .data(),
        perpetua::accounts::CreateMarket {
            authority: *user,
            market,
            vault,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

// F3+F2: resolve prediction market (permissionless, resolver siapa pun) dari akun harga Pyth.
fn ix_resolve(program_id: Pubkey, resolver: &Pubkey, market: Pubkey, price_update: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        program_id,
        &perpetua::instruction::ResolveMarket {}.data(),
        perpetua::accounts::ResolveMarket {
            resolver: *resolver,
            market,
            price_update,
        }
        .to_account_metas(None),
    )
}

// F2: dorong harga Pyth terverifikasi ke mark (permissionless, keeper siapa pun).
fn ix_update_mark(program_id: Pubkey, cranker: &Pubkey, market: Pubkey, price_update: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        program_id,
        &perpetua::instruction::UpdateMarkFromPyth {}.data(),
        perpetua::accounts::UpdateMarkFromPyth {
            cranker: *cranker,
            market,
            price_update,
        }
        .to_account_metas(None),
    )
}

fn ix_open(program_id: Pubkey, user: &Pubkey, market: Pubkey, vault: Pubkey, position: Pubkey, collateral: u64, leverage: u64, is_long: bool) -> Instruction {
    Instruction::new_with_bytes(
        program_id,
        &perpetua::instruction::OpenPosition {
            collateral,
            leverage,
            is_long,
        }
        .data(),
        perpetua::accounts::OpenPosition {
            trader: *user,
            market,
            vault,
            position,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn ix_set_price(program_id: Pubkey, user: &Pubkey, market: Pubkey, price: u64) -> Instruction {
    Instruction::new_with_bytes(
        program_id,
        &perpetua::instruction::SetMarkPrice { price }.data(),
        perpetua::accounts::SetMarkPrice {
            market,
            authority: *user,
        }
        .to_account_metas(None),
    )
}

/// Jalur demo penuh H-1: nyalakan pasar -> buka LONG 5x -> harga +10% -> tutup,
/// payout mencerminkan PnL. Ini jalur yang bakal ditunjukin ke juri (M-02).
#[test]
fn test_perpetua_open_and_close_long_win() {
    let (mut svm, program_id) = setup();
    let user = Keypair::new();
    let feed = Pubkey::new_unique();
    let (market, vault, position) = market_pdas(&program_id, &feed, &user.pubkey());
    svm.airdrop(&user.pubkey(), 50 * SOL).unwrap();

    // 1) NYALAKAN PASAR (bebas-izin), likuiditas awal 10 SOL, harga awal $100.
    send(&mut svm, ix_create_market(program_id, &user.pubkey(), feed, market, vault, PRICE_100, 10 * SOL, SKEW_BIG), &user);
    let m = load_market(&svm, &market);
    assert_eq!(m.authority, user.pubkey());
    assert_eq!(m.oracle_feed, feed);
    assert_eq!(m.mark_price, PRICE_100);
    assert_eq!(m.total_long, 0);
    assert!(svm.get_account(&vault).unwrap().lamports >= 10 * SOL);

    // 2) BUKA LONG 5x dgn jaminan 2 SOL -> notional 10 SOL.
    send(&mut svm, ix_open(program_id, &user.pubkey(), market, vault, position, 2 * SOL, 5, true), &user);
    let p = load_position(&svm, &position);
    assert!(p.is_long);
    assert_eq!(p.collateral, 2 * SOL);
    assert_eq!(p.size, 10 * SOL);
    assert_eq!(p.entry_price, PRICE_100);
    assert_eq!(load_market(&svm, &market).total_long, 10 * SOL);

    // 3) HARGA GERAK +10% ($100 -> $110).
    send(&mut svm, ix_set_price(program_id, &user.pubkey(), market, PRICE_110), &user);
    assert_eq!(load_market(&svm, &market).mark_price, PRICE_110);

    // 4) TUTUP POSISI -> PnL = 10 * 10% = 1 SOL, payout = 2 + 1 = 3 SOL.
    let before = svm.get_account(&user.pubkey()).unwrap().lamports;
    let ix = Instruction::new_with_bytes(
        program_id,
        &perpetua::instruction::ClosePosition {}.data(),
        perpetua::accounts::ClosePosition {
            trader: user.pubkey(),
            market,
            vault,
            position,
        }
        .to_account_metas(None),
    );
    send(&mut svm, ix, &user);
    let after = svm.get_account(&user.pubkey()).unwrap().lamports;
    assert!(
        after > before + 2_900_000_000,
        "payout PnL tidak diterima: before={} after={}",
        before,
        after
    );
    let closed = svm.get_account(&position);
    assert!(closed.is_none() || closed.unwrap().lamports == 0, "posisi belum tertutup");
    assert_eq!(load_market(&svm, &market).total_long, 0);
}

/// H-2: harga jatuh -18% -> margin jebol -> keeper (pihak ketiga) likuidasi,
/// dapat bounty. Ini mekanik "perps beneran" + momen auto-likuidasi buat demo.
#[test]
fn test_perpetua_liquidation_when_margin_breached() {
    let (mut svm, program_id) = setup();
    let user = Keypair::new();
    let liquidator = Keypair::new();
    let feed = Pubkey::new_unique();
    let (market, vault, position) = market_pdas(&program_id, &feed, &user.pubkey());
    svm.airdrop(&user.pubkey(), 50 * SOL).unwrap();
    svm.airdrop(&liquidator.pubkey(), 1 * SOL).unwrap(); // buat bayar fee

    // Setup: pasar $100 + 10 SOL likuiditas, buka LONG 5x jaminan 2 SOL (size 10 SOL).
    send(&mut svm, ix_create_market(program_id, &user.pubkey(), feed, market, vault, PRICE_100, 10 * SOL, SKEW_BIG), &user);
    send(&mut svm, ix_open(program_id, &user.pubkey(), market, vault, position, 2 * SOL, 5, true), &user);

    // Sebelum jatuh: posisi SEHAT -> likuidasi harus DITOLAK.
    let ix_liq = |program_id: Pubkey| Instruction::new_with_bytes(
        program_id,
        &perpetua::instruction::LiquidatePosition {}.data(),
        perpetua::accounts::LiquidatePosition {
            liquidator: liquidator.pubkey(),
            market,
            vault,
            owner: user.pubkey(),
            position,
        }
        .to_account_metas(None),
    );
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix_liq(program_id)], Some(&liquidator.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&liquidator]).unwrap();
    assert!(
        svm.send_transaction(tx).is_err(),
        "posisi sehat mestinya nggak bisa dilikuidasi"
    );

    // HARGA JATUH -18% ($100 -> $82): equity = 2 + 10*(-18%) = 0.2 SOL,
    // maintenance = 10 * 5% = 0.5 SOL -> 0.2 <= 0.5 -> BISA dilikuidasi.
    send(&mut svm, ix_set_price(program_id, &user.pubkey(), market, PRICE_82), &user);

    // Blockhash baru: biar tx likuidasi asli ini nggak byte-identik (→ signature sama)
    // dgn percobaan "sehat" di atas. Tanpa ini litesvm nolak: AlreadyProcessed.
    svm.expire_blockhash();

    let liq_before = svm.get_account(&liquidator.pubkey()).unwrap().lamports;
    send(&mut svm, ix_liq(program_id), &liquidator);
    let liq_after = svm.get_account(&liquidator.pubkey()).unwrap().lamports;

    // Bounty = 10 SOL * 1% = 0.1 SOL. Liquidator naik ~0.1 SOL (dikurangi fee kecil).
    assert!(
        liq_after > liq_before + 90_000_000,
        "bounty likuidasi tidak masuk: before={} after={}",
        liq_before,
        liq_after
    );
    // Posisi ketutup, exposure balik 0.
    let closed = svm.get_account(&position);
    assert!(closed.is_none() || closed.unwrap().lamports == 0, "posisi belum tertutup");
    assert_eq!(load_market(&svm, &market).total_long, 0);
}

/// Demo default (DEMO.md Babak 2) PASCA-SKEW: seed 2 SOL, LONG 0.5 SOL 5x (size 2.5),
/// skew_scale 25 SOL -> entry skew $157.50, pump +25% ke $187.50, TUTUP. Harga keluar ikut skew:
/// 187.5 * 1.05 = $196.875 -> PnL = 2.5*(196.875-157.5)/157.5 = 0.625 SOL -> payout 1.125 SOL. Vault 2.5 SOL = solvent.
#[test]
fn test_demo_defaults_pump_then_close() {
    let (mut svm, program_id) = setup();
    let user = Keypair::new();
    let feed = Pubkey::new_unique();
    let (market, vault, position) = market_pdas(&program_id, &feed, &user.pubkey());
    svm.airdrop(&user.pubkey(), 50 * SOL).unwrap();

    const PRICE_150: u64 = 150_000_000; // harga awal demo (SOL/USD fallback)
    const PRICE_187_5: u64 = 187_500_000; // +25% (tombol pump)
    let half = SOL / 2; // 0.5 SOL collateral (default demo)
    let seed = 2 * SOL; // likuiditas awal (fix 4 Okt: 0.5 SOL kurang buat payout +125%)

    // IGNITE: seed 2 SOL (persis App.tsx createMarket pasca-fix).
    send(&mut svm, ix_create_market(program_id, &user.pubkey(), feed, market, vault, PRICE_150, seed, SKEW_DEMO), &user);
    // OPEN LONG 0.5 SOL 5x -> size 2.5 SOL.
    send(&mut svm, ix_open(program_id, &user.pubkey(), market, vault, position, half, 5, true), &user);
    // Skew fill: LONG 2.5 SOL ke buku kosong, skew_scale 25 SOL -> premium rata2 5%
    // -> entry = 150 * 1.05 = $157.50 (bukan $150 mentah). INI inti skew: size gerakin harga.
    assert_eq!(load_position(&svm, &position).entry_price, 157_500_000);
    // PUMP +25%.
    send(&mut svm, ix_set_price(program_id, &user.pubkey(), market, PRICE_187_5), &user);
    // CLOSE -> payout = 0.5 + 2.5*25% = 1.125 SOL. Harus SUKSES + masuk ke wallet trader.
    let before = svm.get_account(&user.pubkey()).unwrap().lamports;
    let ix = Instruction::new_with_bytes(
        program_id,
        &perpetua::instruction::ClosePosition {}.data(),
        perpetua::accounts::ClosePosition { trader: user.pubkey(), market, vault, position }
            .to_account_metas(None),
    );
    send(&mut svm, ix, &user);
    let after = svm.get_account(&user.pubkey()).unwrap().lamports;
    // CLOSE: harga keluar $196.875 (mark $187.50 + premium skew 5%) vs entry $157.50 -> PnL 0.625 SOL,
    // payout = 0.5 + 0.625 = 1.125 SOL (+ rent refund posisi, - fee). Solvent (vault 2.5 SOL).
    assert!(
        after > before + 1_120_000_000 && after < before + 1_135_000_000,
        "payout skew nggak sesuai: before={} after={}",
        before, after
    );
    assert_eq!(load_market(&svm, &market).total_long, 0);
}

/// AUDIT 4 Okt, SELF-LIQUIDATION (demo Babak 3): di app, burner = owner DAN liquidator
/// (frontend liquidate() ngirim owner=publicKey, liquidator=publicKey). Config ini nggak
/// kena test lama (yg pakai 2 keypair beda), pastiin owner==liquidator nggak bikin error.
#[test]
fn test_demo_self_liquidation() {
    let (mut svm, program_id) = setup();
    let user = Keypair::new(); // burner tunggal: owner SEKALIGUS liquidator
    let feed = Pubkey::new_unique();
    let (market, vault, position) = market_pdas(&program_id, &feed, &user.pubkey());
    svm.airdrop(&user.pubkey(), 50 * SOL).unwrap();

    const PRICE_150: u64 = 150_000_000;
    const PRICE_CRASH: u64 = 112_500_000; // -25% (tombol crash)
    let half = SOL / 2;
    let seed = 2 * SOL;

    send(&mut svm, ix_create_market(program_id, &user.pubkey(), feed, market, vault, PRICE_150, seed, SKEW_DEMO), &user);
    send(&mut svm, ix_open(program_id, &user.pubkey(), market, vault, position, half, 5, true), &user);
    send(&mut svm, ix_set_price(program_id, &user.pubkey(), market, PRICE_CRASH), &user);

    // LIKUIDASI: owner == liquidator == user (persis tombol likuidasi di app).
    let ix = Instruction::new_with_bytes(
        program_id,
        &perpetua::instruction::LiquidatePosition {}.data(),
        perpetua::accounts::LiquidatePosition {
            liquidator: user.pubkey(),
            market,
            vault,
            owner: user.pubkey(),
            position,
        }
        .to_account_metas(None),
    );
    send(&mut svm, ix, &user);

    // Posisi ketutup paksa, exposure balik 0.
    let closed = svm.get_account(&position);
    assert!(closed.is_none() || closed.unwrap().lamports == 0, "posisi belum tertutup");
    assert_eq!(load_market(&svm, &market).total_long, 0);
}

/// B1 skew-premium: UKURAN posisi gerakin harga isi. LONG ke buku kosong -> fill DI ATAS
/// oracle; SHORT ke buku kosong -> fill DI BAWAH oracle. Simetris. (Nol price-impact = MATI.)
#[test]
fn test_skew_premium_long_up_short_down() {
    let (mut svm, program_id) = setup();
    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 50 * SOL).unwrap();
    const PRICE_150: u64 = 150_000_000;
    let depth = 25 * SOL; // skew_scale
    let col = SOL / 2; // 0.5 SOL -> size 2.5 SOL @ 5x

    // LONG ke buku kosong: skew +2.5 SOL, premium rata2 +5% -> entry $157.50.
    let feed_l = Pubkey::new_unique();
    let (m_l, v_l, p_l) = market_pdas(&program_id, &feed_l, &user.pubkey());
    send(&mut svm, ix_create_market(program_id, &user.pubkey(), feed_l, m_l, v_l, PRICE_150, 2 * SOL, depth), &user);
    send(&mut svm, ix_open(program_id, &user.pubkey(), m_l, v_l, p_l, col, 5, true), &user);
    assert_eq!(load_position(&svm, &p_l).entry_price, 157_500_000, "LONG mestinya isi DI ATAS oracle");

    // SHORT ke buku kosong (feed beda): skew -2.5 SOL, premium rata2 -5% -> entry $142.50.
    let feed_s = Pubkey::new_unique();
    let (m_s, v_s, p_s) = market_pdas(&program_id, &feed_s, &user.pubkey());
    send(&mut svm, ix_create_market(program_id, &user.pubkey(), feed_s, m_s, v_s, PRICE_150, 2 * SOL, depth), &user);
    send(&mut svm, ix_open(program_id, &user.pubkey(), m_s, v_s, p_s, col, 5, false), &user);
    assert_eq!(load_position(&svm, &p_s).entry_price, 142_500_000, "SHORT mestinya isi DI BAWAH oracle");
}

/// Harga keluar ikut skew (cermin harga isi): buka lalu LANGSUNG tutup di pasar sepi = IMPAS PERSIS, bukan
/// rugi premium. LONG & SHORT 0.5 SOL 5x di depth 25 SOL: isi 5% di atas/bawah mark, keluar juga di situ
/// -> payout = jaminan 0.5 SOL persis (+ rent akun posisi balik, - fee 1 signature).
#[test]
fn test_round_trip_is_flat() {
    let (mut svm, program_id) = setup();
    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 50 * SOL).unwrap();
    let rent = svm.minimum_balance_for_rent_exemption(8 + <perpetua::state::Position as anchor_lang::Space>::INIT_SPACE);
    for is_long in [true, false] {
        let feed = Pubkey::new_unique();
        let (market, vault, position) = market_pdas(&program_id, &feed, &user.pubkey());
        send(&mut svm, ix_create_market(program_id, &user.pubkey(), feed, market, vault, 150_000_000, 2 * SOL, SKEW_DEMO), &user);
        send(&mut svm, ix_open(program_id, &user.pubkey(), market, vault, position, SOL / 2, 5, is_long), &user);
        let entry = load_position(&svm, &position).entry_price;
        assert_eq!(entry, if is_long { 157_500_000 } else { 142_500_000 }, "isi mestinya kena premium skew 5%");
        let before = svm.get_account(&user.pubkey()).unwrap().lamports;
        let ix = Instruction::new_with_bytes(
            program_id,
            &perpetua::instruction::ClosePosition {}.data(),
            perpetua::accounts::ClosePosition { trader: user.pubkey(), market, vault, position }.to_account_metas(None),
        );
        send(&mut svm, ix, &user);
        let after = svm.get_account(&user.pubkey()).unwrap().lamports;
        assert_eq!(after + 5_000 - before, SOL / 2 + rent, "buka-tutup instan mestinya impas (is_long={})", is_long);
        assert_eq!(load_market(&svm, &market).total_long + load_market(&svm, &market).total_short, 0);
    }
}

/// B2 funding velocity: pasar LONG-HEAVY -> index funding naik -> LONG BAYAR funding.
/// PnL nol dengan sendirinya (mark nggak gerak, harga keluar = harga isi karena premium skew balik pas
/// tutup) biar efek funding keisolasi. Jam on-chain dimajuin 1 jam.
#[test]
fn test_funding_long_heavy_long_pays() {
    let (mut svm, program_id) = setup();
    let user = Keypair::new();
    let feed = Pubkey::new_unique();
    let (market, vault, position) = market_pdas(&program_id, &feed, &user.pubkey());
    svm.airdrop(&user.pubkey(), 50 * SOL).unwrap();
    const PRICE_150: u64 = 150_000_000;

    send(&mut svm, ix_create_market(program_id, &user.pubkey(), feed, market, vault, PRICE_150, 2 * SOL, SKEW_DEMO), &user);
    send(&mut svm, ix_open(program_id, &user.pubkey(), market, vault, position, SOL / 2, 5, true), &user);
    // Mark tetap $150: isi $157.50, keluar juga $157.50 (premium skew balik) -> PnL 0, sisanya murni funding.

    // Majuin jam on-chain +1 jam -> funding akru (skew long-heavy -> long BAYAR).
    let mut clock: Clock = svm.get_sysvar();
    clock.unix_timestamp += 3600;
    svm.set_sysvar(&clock);

    let before = svm.get_account(&user.pubkey()).unwrap().lamports;
    let ix = Instruction::new_with_bytes(
        program_id,
        &perpetua::instruction::ClosePosition {}.data(),
        perpetua::accounts::ClosePosition { trader: user.pubkey(), market, vault, position }.to_account_metas(None),
    );
    send(&mut svm, ix, &user);
    let after = svm.get_account(&user.pubkey()).unwrap().lamports;
    // PnL=0 tapi long di pasar long-heavy BAYAR funding -> payout < jaminan 0.5 SOL.
    assert!(after < before + 500_000_000, "long mestinya BAYAR funding: before={} after={}", before, after);
    assert!(after > before + 400_000_000, "funding kebanyakan/salah tanda: before={} after={}", before, after);
    // Index funding positif di pasar long-heavy.
    assert!(load_market(&svm, &market).cumulative_funding > 0, "cumulative_funding mestinya > 0");
}

/// F3+F2 SELF-RESOLVING PREDICTION MARKET dari PYTH: pasar prediction ber-expiry. Resolve DITOLAK
/// sebelum jatuh tempo; pas expiry siapa pun (PIHAK KETIGA) boleh resolve -> kontrak BACA harga
/// Pyth terverifikasi & kunci; abis itu harganya nggak bisa digerakin siapa pun (pembuat pasar &
/// keeper ditolak); close settle di harga TERKUNCI; resolve kedua ditolak. Nggak ada manusia/komite
/// yg mutusin, Pyth hakimnya.
#[test]
fn test_prediction_resolves_from_pyth() {
    let (mut svm, program_id) = setup();
    let trader = Keypair::new();
    let resolver = Keypair::new(); // PIHAK KETIGA, bukan authority/owner. Bebas-izin.
    let feed = Pubkey::new_unique();
    let pyth = Pubkey::new_unique();
    svm.airdrop(&trader.pubkey(), 50 * SOL).unwrap();
    svm.airdrop(&resolver.pubkey(), 1 * SOL).unwrap();

    const PRICE_180: u64 = 180_000_000; // +20%: harga Pyth pas expiry
    let clock: Clock = svm.get_sysvar();
    let expiry = clock.unix_timestamp + 3600; // jatuh tempo 1 jam lagi
    let (market, vault, position) = market_pdas_exp(&program_id, &feed, &trader.pubkey(), expiry);

    // IGNITE prediction market (kind=1, expiry 1 jam): harga $100, likuiditas 20 SOL, depth raksasa.
    send(&mut svm, ix_create_market_kind(program_id, &trader.pubkey(), feed, market, vault, PRICE_100, 20 * SOL, SKEW_BIG, perpetua::constants::MARKET_KIND_PREDICTION, expiry), &trader);
    let m = load_market(&svm, &market);
    assert_eq!(m.market_kind, perpetua::constants::MARKET_KIND_PREDICTION);
    assert_eq!(m.expiry_ts, expiry);
    assert!(!m.resolved);

    // BET: LONG 5x jaminan 2 SOL -> size 10 SOL (= taruhan harga NAIK). entry = $100 (depth raksasa).
    send(&mut svm, ix_open(program_id, &trader.pubkey(), market, vault, position, 2 * SOL, 5, true), &trader);
    assert_eq!(load_position(&svm, &position).entry_price, PRICE_100);

    // Pyth terbitin $180 lima detik abis jatuh tempo (terverifikasi Full).
    put_pyth(&mut svm, pyth, pyth_data(&feed, pyth_usd(180), 9_000_000, -8, expiry + 5, true), perpetua::constants::PYTH_RECEIVER_PROGRAM_ID);

    // RESOLVE SEBELUM JATUH TEMPO -> DITOLAK (NotExpired).
    expect_err(&mut svm, ix_resolve(program_id, &resolver.pubkey(), market, pyth), &resolver, err_code(perpetua::error::PerpetuaError::NotExpired), "resolve sebelum expiry");

    // Majuin jam on-chain lewat jatuh tempo.
    set_clock(&mut svm, expiry + 10);

    // RESOLVE oleh PIHAK KETIGA (permissionless) -> kontrak baca Pyth, kunci $180.
    send(&mut svm, ix_resolve(program_id, &resolver.pubkey(), market, pyth), &resolver);
    let m2 = load_market(&svm, &market);
    assert!(m2.resolved, "market mestinya resolved");
    assert_eq!(m2.resolution_price, PRICE_180, "harga Pyth pas expiry mestinya TERKUNCI");
    assert_eq!(m2.last_oracle_ts, expiry + 5, "publish_time Pyth mestinya kecatat");

    // RESOLVE LAGI -> DITOLAK (AlreadyResolved).
    expect_err(&mut svm, ix_resolve(program_id, &resolver.pubkey(), market, pyth), &resolver, err_code(perpetua::error::PerpetuaError::AlreadyResolved), "resolve kedua");

    // BUKTI KUNCI: abis resolve NGGAK ADA yang bisa gerakin harga, pembuat pasar ditolak (prediction
    // = harga cuma dari Pyth), keeper Pyth juga ditolak (udah resolved).
    expect_err(&mut svm, ix_set_price(program_id, &trader.pubkey(), market, PRICE_100), &trader, err_code(perpetua::error::PerpetuaError::CreatorPriceOnPrediction), "override pembuat pasar abis resolve");
    expect_err(&mut svm, ix_update_mark(program_id, &resolver.pubkey(), market, pyth), &resolver, err_code(perpetua::error::PerpetuaError::AlreadyResolved), "dorong Pyth abis resolve");

    // CLOSE -> settle di harga RESOLUSI $180 terkunci. PnL = 10*(180-100)/100 = 8 SOL,
    // payout = 2 + 8 = 10 SOL. Vault 20 + 2 jaminan = 22 -> solvent.
    let before = svm.get_account(&trader.pubkey()).unwrap().lamports;
    let ix = Instruction::new_with_bytes(
        program_id,
        &perpetua::instruction::ClosePosition {}.data(),
        perpetua::accounts::ClosePosition { trader: trader.pubkey(), market, vault, position }.to_account_metas(None),
    );
    send(&mut svm, ix, &trader);
    let after = svm.get_account(&trader.pubkey()).unwrap().lamports;
    // payout ~10 SOL (di harga terkunci $180).
    assert!(
        after > before + 9_900_000_000 && after < before + 10_100_000_000,
        "settle mestinya di harga TERKUNCI $180 (payout ~10 SOL): before={} after={}",
        before, after
    );
    assert_eq!(load_market(&svm, &market).total_long, 0);
}

/// CELAH DITUTUP: pembuat pasar pegang LONG lalu nyoba ngisi mark $180 sendiri pas expiry
/// (set_mark_price) -> DITOLAK on-chain (prediction = harga cuma dari Pyth). Pyth bilang $140.
/// Putusan HARUS $140.
#[test]
fn test_resolve_ignores_creator_mark() {
    let (mut svm, program_id) = setup();
    let creator = Keypair::new(); // authority pasar SEKALIGUS trader, calon curang
    let resolver = Keypair::new();
    let feed = Pubkey::new_unique();
    let pyth = Pubkey::new_unique();
    svm.airdrop(&creator.pubkey(), 50 * SOL).unwrap();
    svm.airdrop(&resolver.pubkey(), 1 * SOL).unwrap();

    let expiry = svm.get_sysvar::<Clock>().unix_timestamp + 3600;
    let (market, vault, position) = market_pdas_exp(&program_id, &feed, &creator.pubkey(), expiry);
    send(&mut svm, ix_create_market_kind(program_id, &creator.pubkey(), feed, market, vault, PRICE_100, 20 * SOL, SKEW_BIG, perpetua::constants::MARKET_KIND_PREDICTION, expiry), &creator);
    send(&mut svm, ix_open(program_id, &creator.pubkey(), market, vault, position, 2 * SOL, 5, true), &creator);

    // Pas expiry: pembuat pasar nyoba ngarang $180 -> DITOLAK. Pyth asli bilang $140.
    set_clock(&mut svm, expiry + 10);
    expect_err(&mut svm, ix_set_price(program_id, &creator.pubkey(), market, 180_000_000), &creator, err_code(perpetua::error::PerpetuaError::CreatorPriceOnPrediction), "pembuat pasar ngarang harga prediction");
    assert_eq!(load_market(&svm, &market).mark_price, PRICE_100, "mark nggak boleh kegeser karangan pembuat pasar");
    put_pyth(&mut svm, pyth, pyth_data(&feed, pyth_usd(140), 7_000_000, -8, expiry + 5, true), perpetua::constants::PYTH_RECEIVER_PROGRAM_ID);

    send(&mut svm, ix_resolve(program_id, &resolver.pubkey(), market, pyth), &resolver);
    assert_eq!(load_market(&svm, &market).resolution_price, 140_000_000, "putusan mestinya harga PYTH $140, bukan karangan $180");

    // CLOSE -> PnL = 10*(140-100)/100 = 4 SOL, payout = 2 + 4 = 6 SOL (BUKAN 10 SOL versi curang).
    let before = svm.get_account(&creator.pubkey()).unwrap().lamports;
    let ix = Instruction::new_with_bytes(
        program_id,
        &perpetua::instruction::ClosePosition {}.data(),
        perpetua::accounts::ClosePosition { trader: creator.pubkey(), market, vault, position }.to_account_metas(None),
    );
    send(&mut svm, ix, &creator);
    let after = svm.get_account(&creator.pubkey()).unwrap().lamports;
    assert!(
        after > before + 5_900_000_000 && after < before + 6_100_000_000,
        "payout mestinya ~6 SOL (harga Pyth $140): before={} after={}",
        before, after
    );
}

/// Akun harga yang nggak bener HARUS ditolak, tiap penolakan dicek KODE error-nya:
/// palsu (owner bukan Pyth), feed lain, verifikasi Partial, terbit sebelum expiry, kelewat
/// jendela, conf kelebaran. Terakhir yang bener lolos (bukti setup-nya sah).
#[test]
fn test_resolve_rejects_bad_oracle() {
    use perpetua::error::PerpetuaError as E;
    let (mut svm, program_id) = setup();
    let creator = Keypair::new();
    let resolver = Keypair::new();
    let feed = Pubkey::new_unique();
    let other_feed = Pubkey::new_unique();
    let pyth = Pubkey::new_unique();
    svm.airdrop(&creator.pubkey(), 50 * SOL).unwrap();
    svm.airdrop(&resolver.pubkey(), 1 * SOL).unwrap();

    let expiry = svm.get_sysvar::<Clock>().unix_timestamp + 3600;
    let (market, vault, _position) = market_pdas_exp(&program_id, &feed, &creator.pubkey(), expiry);
    send(&mut svm, ix_create_market_kind(program_id, &creator.pubkey(), feed, market, vault, PRICE_100, 20 * SOL, SKEW_BIG, perpetua::constants::MARKET_KIND_PREDICTION, expiry), &creator);
    set_clock(&mut svm, expiry + 10);

    let receiver = perpetua::constants::PYTH_RECEIVER_PROGRAM_ID;
    let window = perpetua::constants::RESOLVE_WINDOW_SECONDS;
    let good = |f: &Pubkey, publish: i64| pyth_data(f, pyth_usd(150), 1_000_000, -8, publish, true);
    let resolve = |pk: Pubkey| ix_resolve(program_id, &resolver.pubkey(), market, pk);

    // 1) Akun PALSU: isinya mirip, tapi owner program lain (bisa dibikin siapa aja).
    put_pyth(&mut svm, pyth, good(&feed, expiry + 5), Pubkey::new_unique());
    expect_err(&mut svm, resolve(pyth), &resolver, err_code(E::OracleInvalidAccount), "owner palsu");
    // 2) Harga feed LAIN.
    put_pyth(&mut svm, pyth, good(&other_feed, expiry + 5), receiver);
    expect_err(&mut svm, resolve(pyth), &resolver, err_code(E::OracleFeedMismatch), "feed lain");
    // 3) Verifikasi Partial (nggak semua guardian).
    put_pyth(&mut svm, pyth, pyth_data(&feed, pyth_usd(150), 1_000_000, -8, expiry + 5, false), receiver);
    expect_err(&mut svm, resolve(pyth), &resolver, err_code(E::OracleNotFullyVerified), "Partial");
    // 4) Harga terbit SEBELUM expiry (harga lama nggak boleh jadi putusan).
    put_pyth(&mut svm, pyth, good(&feed, expiry - 1), receiver);
    expect_err(&mut svm, resolve(pyth), &resolver, err_code(E::OraclePriceBeforeExpiry), "sebelum expiry");
    // 5) Harga terbit LEWAT jendela resolusi (nunggu harga enak).
    put_pyth(&mut svm, pyth, good(&feed, expiry + window + 1), receiver);
    expect_err(&mut svm, resolve(pyth), &resolver, err_code(E::OraclePriceTooLate), "lewat jendela");
    // 6) Conf kelebaran: ±2% dari harga (batas 1%).
    put_pyth(&mut svm, pyth, pyth_data(&feed, pyth_usd(150), 300_000_000, -8, expiry + 5, true), receiver);
    expect_err(&mut svm, resolve(pyth), &resolver, err_code(E::OracleConfidenceTooWide), "conf lebar");
    assert!(!load_market(&svm, &market).resolved, "market belum boleh resolved");

    // 7) Yang bener -> LOLOS. Tepat di ujung jendela masih sah.
    put_pyth(&mut svm, pyth, good(&feed, expiry + window), receiver);
    svm.expire_blockhash();
    send(&mut svm, resolve(pyth), &resolver);
    assert_eq!(load_market(&svm, &market).resolution_price, 150_000_000);
}

/// Slot pasar = (feed, jatuh tempo): satu feed bisa punya perp + BANYAK ronde prediksi. Ronde 1 resolve,
/// ronde 2 tetep jalan & bisa dibuka posisi baru, feed nggak "mati" abis satu ronde. Perp wajib expiry 0.
#[test]
fn test_prediction_rounds_per_feed() {
    use perpetua::error::PerpetuaError as E;
    let (mut svm, program_id) = setup();
    let user = Keypair::new();
    let feed = Pubkey::new_unique();
    let pyth = Pubkey::new_unique();
    svm.airdrop(&user.pubkey(), 100 * SOL).unwrap();
    let now = svm.get_sysvar::<Clock>().unix_timestamp;
    let (e1, e2) = (now + 60, now + 3600);
    let kind = perpetua::constants::MARKET_KIND_PREDICTION;

    // Perp + 2 ronde prediksi di feed yang SAMA -> 3 alamat beda, semua sah.
    let (mp, vp, _pp) = market_pdas(&program_id, &feed, &user.pubkey());
    let (m1, v1, p1) = market_pdas_exp(&program_id, &feed, &user.pubkey(), e1);
    let (m2, v2, p2) = market_pdas_exp(&program_id, &feed, &user.pubkey(), e2);
    assert!(mp != m1 && m1 != m2 && mp != m2, "tiap slot mestinya alamat beda");
    send(&mut svm, ix_create_market(program_id, &user.pubkey(), feed, mp, vp, PRICE_100, 2 * SOL, SKEW_BIG), &user);
    send(&mut svm, ix_create_market_kind(program_id, &user.pubkey(), feed, m1, v1, PRICE_100, 5 * SOL, SKEW_BIG, kind, e1), &user);
    send(&mut svm, ix_create_market_kind(program_id, &user.pubkey(), feed, m2, v2, PRICE_100, 5 * SOL, SKEW_BIG, kind, e2), &user);
    assert_eq!(load_market(&svm, &m1).expiry_ts, e1);
    assert_eq!(load_market(&svm, &m2).expiry_ts, e2);
    assert_eq!(load_market(&svm, &mp).expiry_ts, 0);

    // Perp ber-expiry -> ditolak (expiry = seed alamat, perp harus 0).
    let other = Pubkey::new_unique();
    let (mx, vx, _) = market_pdas_exp(&program_id, &other, &user.pubkey(), e1);
    expect_err(&mut svm, ix_create_market_kind(program_id, &user.pubkey(), other, mx, vx, PRICE_100, SOL, SKEW_BIG, perpetua::constants::MARKET_KIND_PERP, e1), &user, err_code(E::PerpHasNoExpiry), "perp ber-expiry");

    // Ronde 1: buka posisi, jatuh tempo, Pyth $120 -> resolve.
    send(&mut svm, ix_open(program_id, &user.pubkey(), m1, v1, p1, SOL, 2, true), &user);
    set_clock(&mut svm, e1 + 10);
    put_pyth(&mut svm, pyth, pyth_data(&feed, pyth_usd(120), 1_000_000, -8, e1 + 5, true), perpetua::constants::PYTH_RECEIVER_PROGRAM_ID);
    send(&mut svm, ix_resolve(program_id, &user.pubkey(), m1, pyth), &user);
    assert!(load_market(&svm, &m1).resolved);
    assert_eq!(load_market(&svm, &m1).resolution_price, 120_000_000);

    // Ronde 2 di feed yang sama tetep hidup: posisi baru masuk, resolve ditolak sebelum jatuh tempo.
    assert!(!load_market(&svm, &m2).resolved);
    send(&mut svm, ix_open(program_id, &user.pubkey(), m2, v2, p2, SOL, 2, false), &user);
    assert_eq!(load_position(&svm, &p2).collateral, SOL);
    expect_err(&mut svm, ix_resolve(program_id, &user.pubkey(), m2, pyth), &user, err_code(E::NotExpired), "ronde 2 belum jatuh tempo");
}

/// F3: resolve HANYA buat prediction market. Perp (kind=0) ditolak (NotPredictionMarket).
#[test]
fn test_resolve_rejects_perp() {
    let (mut svm, program_id) = setup();
    let user = Keypair::new();
    let resolver = Keypair::new();
    let feed = Pubkey::new_unique();
    let pyth = Pubkey::new_unique();
    let (market, vault, _position) = market_pdas(&program_id, &feed, &user.pubkey());
    svm.airdrop(&user.pubkey(), 50 * SOL).unwrap();
    svm.airdrop(&resolver.pubkey(), 1 * SOL).unwrap();

    // Pasar PERP biasa (kind=0, expiry=0). Akun Pyth-nya sah, yang salah jenis pasarnya.
    send(&mut svm, ix_create_market(program_id, &user.pubkey(), feed, market, vault, PRICE_100, 10 * SOL, SKEW_BIG), &user);
    let now = svm.get_sysvar::<Clock>().unix_timestamp;
    put_pyth(&mut svm, pyth, pyth_data(&feed, pyth_usd(100), 1_000_000, -8, now, true), perpetua::constants::PYTH_RECEIVER_PROGRAM_ID);
    expect_err(&mut svm, ix_resolve(program_id, &resolver.pubkey(), market, pyth), &resolver, err_code(perpetua::error::PerpetuaError::NotPredictionMarket), "resolve perp");
}

/// F2: keeper SIAPA PUN (bukan authority) dorong harga Pyth -> mark ke-skala 6 desimal +
/// publish_time kecatat. Harga basi (>120 dtk) & harga lebih lama dari yg terakhir ditolak.
/// Override demo pembuat pasar boleh SEBELUM ada harga Pyth; begitu harga Pyth masuk, override
/// PENSIUN (ditolak) dan harga Pyth + badge-nya utuh.
#[test]
fn test_update_mark_from_pyth_permissionless() {
    use perpetua::error::PerpetuaError as E;
    let (mut svm, program_id) = setup();
    let creator = Keypair::new();
    let keeper = Keypair::new(); // bukan authority
    let feed = Pubkey::new_unique();
    let pyth = Pubkey::new_unique();
    let (market, vault, _position) = market_pdas(&program_id, &feed, &creator.pubkey());
    svm.airdrop(&creator.pubkey(), 50 * SOL).unwrap();
    svm.airdrop(&keeper.pubkey(), 1 * SOL).unwrap();
    let receiver = perpetua::constants::PYTH_RECEIVER_PROGRAM_ID;
    // Jam litesvm mulai dari 0, pasang jam realistis biar publish_time nggak negatif.
    let t: i64 = 1_790_000_000;
    set_clock(&mut svm, t);

    send(&mut svm, ix_create_market(program_id, &creator.pubkey(), feed, market, vault, PRICE_100, 10 * SOL, SKEW_BIG), &creator);
    assert_eq!(load_market(&svm, &market).last_oracle_ts, 0, "mark awal BUKAN dari Pyth");

    // SEBELUM ada harga Pyth: override demo pembuat pasar masih boleh (jalur pump/crash localnet).
    send(&mut svm, ix_set_price(program_id, &creator.pubkey(), market, 101_000_000), &creator);
    assert_eq!(load_market(&svm, &market).mark_price, 101_000_000);

    // Pyth $150 (expo -8), terbit 5 dtk lalu -> mark = 150_000_000.
    put_pyth(&mut svm, pyth, pyth_data(&feed, pyth_usd(150), 1_000_000, -8, t - 5, true), receiver);
    send(&mut svm, ix_update_mark(program_id, &keeper.pubkey(), market, pyth), &keeper);
    let m = load_market(&svm, &market);
    assert_eq!(m.mark_price, 150_000_000);
    assert_eq!(m.last_oracle_ts, t - 5);

    // BASI: jam maju 200 dtk, akun masih yang lama (umur 205 dtk > 120).
    set_clock(&mut svm, t + 200);
    expect_err(&mut svm, ix_update_mark(program_id, &keeper.pubkey(), market, pyth), &keeper, err_code(E::StaleOracle), "harga basi");

    // Seger lagi: terbit t+190 -> lolos.
    put_pyth(&mut svm, pyth, pyth_data(&feed, pyth_usd(152), 1_000_000, -8, t + 190, true), receiver);
    send(&mut svm, ix_update_mark(program_id, &keeper.pubkey(), market, pyth), &keeper);
    assert_eq!(load_market(&svm, &market).mark_price, 152_000_000);

    // MUNDUR: harga terbit t+150 (masih seger, tapi lebih lama dari t+190 yg udah kepake) -> ditolak.
    put_pyth(&mut svm, pyth, pyth_data(&feed, pyth_usd(149), 1_000_000, -8, t + 150, true), receiver);
    expect_err(&mut svm, ix_update_mark(program_id, &keeper.pubkey(), market, pyth), &keeper, err_code(E::StaleOracle), "harga mundur");

    // Pasar udah punya harga Pyth -> override demo pembuat pasar PENSIUN: ditolak, harga Pyth utuh.
    expect_err(&mut svm, ix_set_price(program_id, &creator.pubkey(), market, 120_000_000), &creator, err_code(E::CreatorPriceRetired), "override abis ada harga Pyth");
    let m2 = load_market(&svm, &market);
    assert_eq!(m2.mark_price, 152_000_000, "harga Pyth nggak boleh ketimpa karangan");
    assert_eq!(m2.last_oracle_ts, t + 190, "badge Pyth tetep (publish_time terakhir)");
}

/// Parser kontrak vs BYTE ASLI akun Pyth devnet (SOL/USD, owner Pyth Receiver): bukan fixture
/// karangan. Kalau discriminator/offset/skala salah, test ini merah.
/// price 10745547506 expo -8 -> $107.45547506 -> 6 desimal 107_455_475.
#[test]
fn test_update_mark_from_real_pyth_bytes() {
    let (mut svm, program_id) = setup();
    let creator = Keypair::new();
    let keeper = Keypair::new();
    let mut feed_bytes = [0u8; 32];
    feed_bytes.copy_from_slice(&REAL_PYTH_SOL_USD[41..73]);
    let feed = Pubkey::new_from_array(feed_bytes); // = feed id SOL/USD ef0d8b6f…
    let pyth = Pubkey::new_unique();
    let (market, vault, _position) = market_pdas(&program_id, &feed, &creator.pubkey());
    svm.airdrop(&creator.pubkey(), 50 * SOL).unwrap();
    svm.airdrop(&keeper.pubkey(), 1 * SOL).unwrap();

    send(&mut svm, ix_create_market(program_id, &creator.pubkey(), feed, market, vault, PRICE_100, 10 * SOL, SKEW_BIG), &creator);
    set_clock(&mut svm, REAL_PYTH_PUBLISH_TIME + 30);
    put_pyth(&mut svm, pyth, REAL_PYTH_SOL_USD.to_vec(), perpetua::constants::PYTH_RECEIVER_PROGRAM_ID);

    send(&mut svm, ix_update_mark(program_id, &keeper.pubkey(), market, pyth), &keeper);
    let m = load_market(&svm, &market);
    assert_eq!(m.mark_price, 107_455_475, "harga SOL/USD asli mestinya kebaca $107.455475");
    assert_eq!(m.last_oracle_ts, REAL_PYTH_PUBLISH_TIME);
}
