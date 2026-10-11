use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{clock::Clock, instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
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

/// Kirim tx yang BOLEH gagal; return true kalau sukses. Buat ngetes jalur yang mesti DITOLAK.
fn try_send(svm: &mut LiteSVM, ix: Instruction, payer: &Keypair) -> bool {
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]).unwrap();
    svm.send_transaction(tx).is_ok()
}

fn market_pdas(program_id: &Pubkey, feed: &Pubkey, user: &Pubkey) -> (Pubkey, Pubkey, Pubkey) {
    let (market, _) =
        Pubkey::find_program_address(&[perpetua::constants::MARKET_SEED, feed.as_ref()], program_id);
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

// Perp default (kind=0, expiry=0) — 6 test lama manggil ini, NOL berubah.
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

// F3: resolve prediction market (permissionless — resolver siapa pun).
fn ix_resolve(program_id: Pubkey, resolver: &Pubkey, market: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        program_id,
        &perpetua::instruction::ResolveMarket {}.data(),
        perpetua::accounts::ResolveMarket {
            resolver: *resolver,
            market,
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

    // 1) NYALAKAN PASAR (bebas-izin) — likuiditas awal 10 SOL, harga awal $100.
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
/// skew_scale 25 SOL -> entry skew $157.50, pump +25% ke $187.50, TUTUP.
/// PnL = 2.5*(187.5-157.5)/157.5 = ~0.476 SOL -> payout ~0.976 SOL. Vault 2.5 SOL = solvent.
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
    // CLOSE: mark $187.50 vs entry skew $157.50 -> PnL = 2.5*(187.5-157.5)/157.5 = ~0.476 SOL,
    // payout = 0.5 + 0.476 = ~0.976 SOL (+ rent refund posisi, - fee). Solvent (vault 2.5 SOL).
    assert!(
        after > before + 950_000_000 && after < before + 1_010_000_000,
        "payout skew nggak sesuai: before={} after={}",
        before, after
    );
    assert_eq!(load_market(&svm, &market).total_long, 0);
}

/// AUDIT 4 Okt — SELF-LIQUIDATION (demo Babak 3): di app, burner = owner DAN liquidator
/// (frontend liquidate() ngirim owner=publicKey, liquidator=publicKey). Config ini nggak
/// kena test lama (yg pakai 2 keypair beda) — pastiin owner==liquidator nggak bikin error.
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

/// B2 funding velocity: pasar LONG-HEAVY -> index funding naik -> LONG BAYAR funding.
/// PnL di-nol-in (mark = entry skew) biar efek funding keisolasi. Jam on-chain dimajuin 1 jam.
#[test]
fn test_funding_long_heavy_long_pays() {
    let (mut svm, program_id) = setup();
    let user = Keypair::new();
    let feed = Pubkey::new_unique();
    let (market, vault, position) = market_pdas(&program_id, &feed, &user.pubkey());
    svm.airdrop(&user.pubkey(), 50 * SOL).unwrap();
    const PRICE_150: u64 = 150_000_000;
    const ENTRY_SKEW: u64 = 157_500_000; // fill LONG 2.5 SOL @ depth 25 SOL

    send(&mut svm, ix_create_market(program_id, &user.pubkey(), feed, market, vault, PRICE_150, 2 * SOL, SKEW_DEMO), &user);
    send(&mut svm, ix_open(program_id, &user.pubkey(), market, vault, position, SOL / 2, 5, true), &user);
    // Nol-in PnL: mark = entry skew ($157.50) -> ekuitas = jaminan, sisanya murni funding.
    send(&mut svm, ix_set_price(program_id, &user.pubkey(), market, ENTRY_SKEW), &user);

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

/// F3 SELF-RESOLVING PREDICTION MARKET: pasar prediction ber-expiry. Resolve DITOLAK sebelum
/// jatuh tempo; pas expiry siapa pun (PIHAK KETIGA) boleh resolve -> kunci harga feed; close
/// settle di harga TERKUNCI meski live mark berubah; resolve kedua ditolak. Inti wedge:
/// nggak ada manusia/komite yg mutusin — feed-nya (Pyth via keeper) hakimnya.
#[test]
fn test_prediction_self_resolve() {
    let (mut svm, program_id) = setup();
    let trader = Keypair::new();
    let resolver = Keypair::new(); // PIHAK KETIGA — bukan authority/owner. Bebas-izin.
    let feed = Pubkey::new_unique();
    let (market, vault, position) = market_pdas(&program_id, &feed, &trader.pubkey());
    svm.airdrop(&trader.pubkey(), 50 * SOL).unwrap();
    svm.airdrop(&resolver.pubkey(), 1 * SOL).unwrap();

    const PRICE_180: u64 = 180_000_000; // +20% harga feed pas expiry
    let clock: Clock = svm.get_sysvar();
    let expiry = clock.unix_timestamp + 3600; // jatuh tempo 1 jam lagi

    // IGNITE prediction market (kind=1, expiry 1 jam): harga $100, likuiditas 20 SOL, depth raksasa.
    send(&mut svm, ix_create_market_kind(program_id, &trader.pubkey(), feed, market, vault, PRICE_100, 20 * SOL, SKEW_BIG, perpetua::constants::MARKET_KIND_PREDICTION, expiry), &trader);
    let m = load_market(&svm, &market);
    assert_eq!(m.market_kind, perpetua::constants::MARKET_KIND_PREDICTION);
    assert_eq!(m.expiry_ts, expiry);
    assert!(!m.resolved);

    // BET: LONG 5x jaminan 2 SOL -> size 10 SOL (= taruhan harga NAIK). entry = $100 (depth raksasa).
    send(&mut svm, ix_open(program_id, &trader.pubkey(), market, vault, position, 2 * SOL, 5, true), &trader);
    assert_eq!(load_position(&svm, &position).entry_price, PRICE_100);

    // RESOLVE SEBELUM JATUH TEMPO -> HARUS DITOLAK (NotExpired).
    assert!(!try_send(&mut svm, ix_resolve(program_id, &resolver.pubkey(), market), &resolver), "resolve sebelum expiry mestinya DITOLAK");

    // Majuin jam on-chain ke jatuh tempo. Keeper dorong harga feed +20% -> $180.
    let mut clock2: Clock = svm.get_sysvar();
    clock2.unix_timestamp = expiry;
    svm.set_sysvar(&clock2);
    send(&mut svm, ix_set_price(program_id, &trader.pubkey(), market, PRICE_180), &trader);
    svm.expire_blockhash(); // biar tx resolve asli nggak byte-identik dgn percobaan gagal di atas

    // RESOLVE oleh PIHAK KETIGA (permissionless) -> kunci resolution_price = $180.
    send(&mut svm, ix_resolve(program_id, &resolver.pubkey(), market), &resolver);
    let m2 = load_market(&svm, &market);
    assert!(m2.resolved, "market mestinya resolved");
    assert_eq!(m2.resolution_price, PRICE_180, "harga feed pas expiry mestinya TERKUNCI");

    // RESOLVE LAGI -> DITOLAK (AlreadyResolved).
    svm.expire_blockhash();
    assert!(!try_send(&mut svm, ix_resolve(program_id, &resolver.pubkey(), market), &resolver), "resolve kedua mestinya DITOLAK");

    // BUKTI KUNCI: live mark balik turun ke $100, TAPI settle tetap pakai $180 terkunci.
    send(&mut svm, ix_set_price(program_id, &trader.pubkey(), market, PRICE_100), &trader);

    // CLOSE -> settle di harga RESOLUSI $180 (BUKAN live $100). PnL = 10*(180-100)/100 = 8 SOL,
    // payout = 2 + 8 = 10 SOL. Vault 20 + 2 jaminan = 22 -> solvent.
    let before = svm.get_account(&trader.pubkey()).unwrap().lamports;
    let ix = Instruction::new_with_bytes(
        program_id,
        &perpetua::instruction::ClosePosition {}.data(),
        perpetua::accounts::ClosePosition { trader: trader.pubkey(), market, vault, position }.to_account_metas(None),
    );
    send(&mut svm, ix, &trader);
    let after = svm.get_account(&trader.pubkey()).unwrap().lamports;
    // payout ~10 SOL (di harga terkunci $180), BUKAN ~2 SOL (kalau salah pakai live $100).
    assert!(
        after > before + 9_900_000_000 && after < before + 10_100_000_000,
        "settle mestinya di harga TERKUNCI $180 (payout ~10 SOL): before={} after={}",
        before, after
    );
    assert_eq!(load_market(&svm, &market).total_long, 0);
}

/// F3: resolve HANYA buat prediction market. Perp (kind=0) ditolak (NotPredictionMarket).
#[test]
fn test_resolve_rejects_perp() {
    let (mut svm, program_id) = setup();
    let user = Keypair::new();
    let resolver = Keypair::new();
    let feed = Pubkey::new_unique();
    let (market, vault, _position) = market_pdas(&program_id, &feed, &user.pubkey());
    svm.airdrop(&user.pubkey(), 50 * SOL).unwrap();
    svm.airdrop(&resolver.pubkey(), 1 * SOL).unwrap();

    // Pasar PERP biasa (kind=0, expiry=0).
    send(&mut svm, ix_create_market(program_id, &user.pubkey(), feed, market, vault, PRICE_100, 10 * SOL, SKEW_BIG), &user);
    // Resolve perp -> DITOLAK (bukan prediction market).
    assert!(!try_send(&mut svm, ix_resolve(program_id, &resolver.pubkey(), market), &resolver), "resolve pada perp mestinya DITOLAK (NotPredictionMarket)");
}
