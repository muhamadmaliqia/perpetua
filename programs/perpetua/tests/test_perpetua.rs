use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
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

fn ix_create_market(program_id: Pubkey, user: &Pubkey, feed: Pubkey, market: Pubkey, vault: Pubkey, price: u64, liq: u64) -> Instruction {
    Instruction::new_with_bytes(
        program_id,
        &perpetua::instruction::CreateMarket {
            oracle_feed: feed,
            initial_price: price,
            initial_liquidity: liq,
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
    send(&mut svm, ix_create_market(program_id, &user.pubkey(), feed, market, vault, PRICE_100, 10 * SOL), &user);
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
    send(&mut svm, ix_create_market(program_id, &user.pubkey(), feed, market, vault, PRICE_100, 10 * SOL), &user);
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

/// AUDIT 4 Okt — replikasi ANGKA DEFAULT DEMO (DEMO.md §2 Babak 2):
/// seed 0.5 SOL likuiditas, buka LONG 0.5 SOL 5x, pump +25%, TUTUP.
/// Payout = 0.5 + 2.5*25% = 1.125 SOL, tapi kolam cuma 0.5+0.5 = 1.0 SOL.
/// Kalau ini panik InsufficientLiquidity = tombol TUTUP bakal "Gagal" di demo.
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
    send(&mut svm, ix_create_market(program_id, &user.pubkey(), feed, market, vault, PRICE_150, seed), &user);
    // OPEN LONG 0.5 SOL 5x -> size 2.5 SOL.
    send(&mut svm, ix_open(program_id, &user.pubkey(), market, vault, position, half, 5, true), &user);
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
    // payout ~1.125 SOL (dikurangi fee kecil, ditambah rent refund posisi) -> naik > 1 SOL.
    assert!(
        after > before + 1_000_000_000,
        "payout +125% nggak masuk: before={} after={}",
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

    send(&mut svm, ix_create_market(program_id, &user.pubkey(), feed, market, vault, PRICE_150, seed), &user);
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
